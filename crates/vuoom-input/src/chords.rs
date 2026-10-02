//! Vuoom's two hotkeys that work while another app has the keyboard: zoom (Ctrl+Shift+Z
//! unless changed) and stop (Ctrl+Shift+X unless changed).
//!
//! Both are the user's to rebind ([`set_chords`]), so everything that reacts to them reads
//! the current chord from here: the input hook, the pollers, the zoom-mark scan and the
//! keystroke overlay's filter.
//!
//! While a recording runs, the low-level keyboard hook ([`crate::InputRecorder`]) sees every
//! key before the focused app does. A press of either chord is counted here and then
//! swallowed, so it never reaches the app being recorded: Ctrl+Shift+Z is Redo in a great
//! many apps, and a zoom must not also redo something in the demo. A swallowed key never
//! shows in the system's key state, so the watchers can't rely on polling alone; and the
//! hook is not called at all while an elevated window has focus, so they can't rely on the
//! hook alone either. [`ChordWatch`] listens both ways and reports each press once.

use std::sync::atomic::{AtomicU32, Ordering};

/// A hotkey: a key with the modifiers held while it is pressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chord {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    /// The key's virtual-key code (a letter, a digit or F1 to F12).
    pub vk: u16,
}

const CTRL_BIT: u32 = 1 << 16;
const SHIFT_BIT: u32 = 1 << 17;
const ALT_BIT: u32 = 1 << 18;
const VK_F1: u16 = 0x70;

impl Chord {
    /// Ctrl+Shift+Z.
    pub const ZOOM_DEFAULT: Self = Self {
        ctrl: true,
        shift: true,
        alt: false,
        vk: 0x5A,
    };
    /// Ctrl+Shift+X.
    pub const STOP_DEFAULT: Self = Self {
        ctrl: true,
        shift: true,
        alt: false,
        vk: 0x58,
    };

    const fn pack(self) -> u32 {
        let mut v = self.vk as u32;
        if self.ctrl {
            v |= CTRL_BIT;
        }
        if self.shift {
            v |= SHIFT_BIT;
        }
        if self.alt {
            v |= ALT_BIT;
        }
        v
    }

    const fn unpack(v: u32) -> Self {
        Self {
            ctrl: v & CTRL_BIT != 0,
            shift: v & SHIFT_BIT != 0,
            alt: v & ALT_BIT != 0,
            vk: (v & 0xFFFF) as u16,
        }
    }

    /// Whether pressing `vk` with exactly these modifiers held is this chord.
    #[must_use]
    pub fn matches(self, ctrl: bool, shift: bool, alt: bool, vk: u16) -> bool {
        vk == self.vk && ctrl == self.ctrl && shift == self.shift && alt == self.alt
    }

    /// Parse a chord written like `"Ctrl+Shift+Z"`, `"Alt+Shift+2"` or `"F9"`.
    ///
    /// The key is a letter, a digit or F1 to F12. A letter or digit needs two modifiers:
    /// with one it would take a shortcut every app uses (Ctrl+C), with none it would take
    /// typing.
    ///
    /// # Errors
    /// Returns a message naming what is wrong with the chord.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut chord = Self {
            ctrl: false,
            shift: false,
            alt: false,
            vk: 0,
        };
        for part in text.split('+').map(str::trim) {
            match part.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => chord.ctrl = true,
                "shift" => chord.shift = true,
                "alt" => chord.alt = true,
                key => {
                    if chord.vk != 0 {
                        return Err(format!("\"{text}\" has more than one key"));
                    }
                    let Some(vk) = key_code(key) else {
                        return Err(format!(
                            "\"{part}\" can't be a hotkey: use a letter, a digit or F1 to F12"
                        ));
                    };
                    chord.vk = vk;
                }
            }
        }
        if chord.vk == 0 {
            return Err(format!("\"{text}\" has no key"));
        }
        let modifiers = [chord.ctrl, chord.shift, chord.alt];
        let held = modifiers.iter().filter(|m| **m).count();
        if chord.vk < VK_F1 && held < 2 {
            return Err("Hold two of Ctrl, Shift and Alt with a letter or digit".into());
        }
        Ok(chord)
    }
}

/// The virtual-key code of a key named like `"z"`, `"7"` or `"f9"` (lower case).
fn key_code(key: &str) -> Option<u16> {
    let bytes = key.as_bytes();
    match bytes {
        [c @ b'a'..=b'z'] => Some(u16::from(c.to_ascii_uppercase())),
        [c @ b'0'..=b'9'] => Some(u16::from(*c)),
        [b'f', digits @ ..] if !digits.is_empty() => {
            let n: u16 = key[1..].parse().ok()?;
            (1..=12).contains(&n).then_some(VK_F1 + n.wrapping_sub(1))
        }
        _ => None,
    }
}

static ZOOM: AtomicU32 = AtomicU32::new(Chord::ZOOM_DEFAULT.pack());
static STOP: AtomicU32 = AtomicU32::new(Chord::STOP_DEFAULT.pack());

/// The chord that zooms while recording.
#[must_use]
pub fn zoom_chord() -> Chord {
    Chord::unpack(ZOOM.load(Ordering::Relaxed))
}

/// The chord that stops the recording.
#[must_use]
pub fn stop_chord() -> Chord {
    Chord::unpack(STOP.load(Ordering::Relaxed))
}

/// Rebind both chords.
///
/// # Errors
/// Refuses two identical chords (one press can't both zoom and stop).
pub fn set_chords(zoom: Chord, stop: Chord) -> Result<(), String> {
    if zoom == stop {
        return Err("Zoom and stop need different hotkeys".into());
    }
    ZOOM.store(zoom.pack(), Ordering::Relaxed);
    STOP.store(stop.pack(), Ordering::Relaxed);
    Ok(())
}

/// One of the two hotkeys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hotkey {
    Zoom,
    Stop,
}

impl Hotkey {
    /// The chord it is bound to right now.
    #[must_use]
    pub fn chord(self) -> Chord {
        match self {
            Self::Zoom => zoom_chord(),
            Self::Stop => stop_chord(),
        }
    }
}

#[cfg(windows)]
pub use os::ChordWatch;
#[cfg(windows)]
pub(crate) use os::{hook_key, hook_reset};

#[cfg(windows)]
mod os {
    use super::{Chord, Hotkey};
    use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
    use std::time::{Duration, Instant};
    use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;

    const VK_SHIFT: i32 = 0x10;
    const VK_CONTROL: i32 = 0x11;
    const VK_MENU: i32 = 0x12;

    /// Presses the keyboard hook counted (and swallowed), per hotkey.
    static ZOOM_PRESSES: AtomicU64 = AtomicU64::new(0);
    static STOP_PRESSES: AtomicU64 = AtomicU64::new(0);
    /// Which hotkeys' keys the hook is holding swallowed (bit 0 zoom, bit 1 stop): their
    /// auto-repeats and their release are swallowed too, so the app never sees half a key.
    static HELD: AtomicU32 = AtomicU32::new(0);

    fn key_down(vk: i32) -> bool {
        // SAFETY: a plain key-state query. The high-order bit is set while the key is down.
        (unsafe { GetAsyncKeyState(vk) } as u16 & 0x8000) != 0
    }

    impl Chord {
        /// Whether exactly this chord's modifiers are held right now.
        fn modifiers_down(self) -> bool {
            key_down(VK_CONTROL) == self.ctrl
                && key_down(VK_SHIFT) == self.shift
                && key_down(VK_MENU) == self.alt
        }

        /// Whether the whole chord is held right now, going by the system's key state.
        #[must_use]
        pub fn is_down(self) -> bool {
            key_down(i32::from(self.vk)) && self.modifiers_down()
        }
    }

    fn presses(hotkey: Hotkey) -> &'static AtomicU64 {
        match hotkey {
            Hotkey::Zoom => &ZOOM_PRESSES,
            Hotkey::Stop => &STOP_PRESSES,
        }
    }

    /// Forget any key the hook was holding (a new hook starts clean).
    pub(crate) fn hook_reset() {
        HELD.store(0, Ordering::Relaxed);
    }

    /// The keyboard hook's question for every key event: is this one of the hotkeys, to be
    /// counted and kept from the focused app? `true` = swallow it.
    pub(crate) fn hook_key(vk: u16, down: bool) -> bool {
        for (bit, hotkey) in [(1u32, Hotkey::Zoom), (2u32, Hotkey::Stop)] {
            let chord = hotkey.chord();
            if vk != chord.vk {
                continue;
            }
            let held = HELD.load(Ordering::Relaxed) & bit != 0;
            if !down {
                if held {
                    HELD.fetch_and(!bit, Ordering::Relaxed);
                }
                // The release goes the way the press went.
                return held;
            }
            if held {
                return true; // auto-repeat of a press already counted
            }
            if chord.modifiers_down() {
                HELD.fetch_or(bit, Ordering::Relaxed);
                presses(hotkey).fetch_add(1, Ordering::Relaxed);
                return true;
            }
        }
        false
    }

    /// Reports each press of a hotkey once, whether the keyboard hook caught it (and
    /// swallowed it) or it only shows in the system's key state (an elevated window has
    /// focus, or no recording's hook is running). Call [`Self::pressed`] every few
    /// milliseconds.
    pub struct ChordWatch {
        hotkey: Hotkey,
        /// The hook's press count when last looked at.
        counted: u64,
        was_down: bool,
        last: Option<Instant>,
    }

    /// A press seen both ways (the hook counted it and the key state shows it) arrives
    /// within a poll or two of itself; anything this close together is one press.
    const SAME_PRESS: Duration = Duration::from_millis(250);

    impl ChordWatch {
        /// Watch `hotkey`. A chord already held when the watch starts must be released
        /// first.
        #[must_use]
        pub fn new(hotkey: Hotkey) -> Self {
            Self {
                hotkey,
                counted: presses(hotkey).load(Ordering::Relaxed),
                was_down: true,
                last: None,
            }
        }

        /// Whether the hotkey was pressed since the last call.
        pub fn pressed(&mut self) -> bool {
            let count = presses(self.hotkey).load(Ordering::Relaxed);
            let hooked = count != self.counted;
            self.counted = count;
            let down = self.hotkey.chord().is_down();
            let polled = down && !self.was_down;
            self.was_down = down;
            if !(hooked || polled) {
                return false;
            }
            let fresh = self.last.is_none_or(|t| t.elapsed() > SAME_PRESS);
            if fresh {
                self.last = Some(Instant::now());
            }
            fresh
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chords_parse_from_how_they_are_written() {
        assert_eq!(Chord::parse("Ctrl+Shift+Z"), Ok(Chord::ZOOM_DEFAULT));
        assert_eq!(Chord::parse(" ctrl + shift + x "), Ok(Chord::STOP_DEFAULT));
        let alt2 = Chord::parse("Alt+Shift+2").unwrap();
        assert!(alt2.matches(false, true, true, 0x32));
        // A function key stands on its own, or with modifiers.
        assert_eq!(Chord::parse("F9").unwrap().vk, 0x78);
        assert_eq!(Chord::parse("Ctrl+F12").unwrap().vk, 0x7B);
    }

    #[test]
    fn weak_or_malformed_chords_are_refused() {
        // A letter with fewer than two modifiers would take over typing or common shortcuts.
        assert!(Chord::parse("Z").is_err());
        assert!(Chord::parse("Ctrl+Z").is_err());
        // No key, two keys, a key that isn't offered.
        assert!(Chord::parse("Ctrl+Shift").is_err());
        assert!(Chord::parse("Ctrl+Shift+A+B").is_err());
        assert!(Chord::parse("Ctrl+Shift+Enter").is_err());
        assert!(Chord::parse("F13").is_err());
        assert!(Chord::parse("F0").is_err());
        assert!(Chord::parse("").is_err());
    }

    #[test]
    fn a_chord_matches_only_its_exact_modifiers() {
        let z = Chord::ZOOM_DEFAULT;
        assert!(z.matches(true, true, false, 0x5A));
        // Ctrl+Z is undo, not zoom.
        assert!(!z.matches(true, false, false, 0x5A));
        assert!(!z.matches(true, true, true, 0x5A));
        assert!(!z.matches(true, true, false, 0x58));
    }

    #[test]
    fn chords_survive_packing() {
        for chord in [
            Chord::ZOOM_DEFAULT,
            Chord::STOP_DEFAULT,
            Chord::parse("Alt+Shift+2").unwrap(),
            Chord::parse("F9").unwrap(),
        ] {
            assert_eq!(Chord::unpack(chord.pack()), chord);
        }
    }

    #[test]
    fn zoom_and_stop_cannot_share_a_chord() {
        assert!(set_chords(Chord::ZOOM_DEFAULT, Chord::ZOOM_DEFAULT).is_err());
        // The refused pair changed nothing.
        assert_eq!(zoom_chord(), Chord::ZOOM_DEFAULT);
        assert_eq!(stop_chord(), Chord::STOP_DEFAULT);
    }
}
