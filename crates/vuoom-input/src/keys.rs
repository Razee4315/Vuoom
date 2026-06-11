//! Virtual-key → human label mapping for the keystroke overlay.
//!
//! Only shortcuts (modifier chords) and "special" keys are ever labeled — plain character
//! typing is deliberately not, both to keep the overlay readable and because echoing typed
//! text into a shared GIF is a privacy hazard.

/// Which modifier a virtual-key code represents, if any.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modifier {
    Shift,
    Ctrl,
    Alt,
    Win,
}

/// Classify a virtual-key code as a modifier.
#[must_use]
pub fn modifier(vk: u16) -> Option<Modifier> {
    match vk {
        0x10 | 0xA0 | 0xA1 => Some(Modifier::Shift),
        0x11 | 0xA2 | 0xA3 => Some(Modifier::Ctrl),
        0x12 | 0xA4 | 0xA5 => Some(Modifier::Alt),
        0x5B | 0x5C => Some(Modifier::Win),
        _ => None,
    }
}

const LETTERS: [&str; 26] = [
    "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R", "S",
    "T", "U", "V", "W", "X", "Y", "Z",
];
const DIGITS: [&str; 10] = ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"];
const FKEYS: [&str; 12] = [
    "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "F11", "F12",
];

/// Human-readable name for a (non-modifier) virtual-key code, if it has one.
#[must_use]
pub fn key_name(vk: u16) -> Option<&'static str> {
    Some(match vk {
        0x08 => "Backspace",
        0x09 => "Tab",
        0x0D => "Enter",
        0x1B => "Esc",
        0x20 => "Space",
        0x21 => "PgUp",
        0x22 => "PgDn",
        0x23 => "End",
        0x24 => "Home",
        0x25 => "←",
        0x26 => "↑",
        0x27 => "→",
        0x28 => "↓",
        0x2D => "Ins",
        0x2E => "Del",
        0x30..=0x39 => DIGITS[(vk - 0x30) as usize],
        0x41..=0x5A => LETTERS[(vk - 0x41) as usize],
        0x70..=0x7B => FKEYS[(vk - 0x70) as usize],
        0xBB => "+",
        0xBC => ",",
        0xBD => "-",
        0xBE => ".",
        0xBF => "/",
        0xC0 => "`",
        _ => return None,
    })
}

/// Whether this key is worth showing without any modifier held (Enter, Esc, F-keys, …).
/// Letters/digits never qualify alone — that would echo typed text.
#[must_use]
pub fn is_standalone(vk: u16) -> bool {
    matches!(vk, 0x09 | 0x0D | 0x1B | 0x2E | 0x25..=0x28 | 0x70..=0x7B)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_and_named_keys_resolve() {
        assert_eq!(key_name(0x41), Some("A"));
        assert_eq!(key_name(0x0D), Some("Enter"));
        assert_eq!(key_name(0x7B), Some("F12"));
        assert_eq!(key_name(0x07), None);
    }

    #[test]
    fn modifiers_classify() {
        assert_eq!(modifier(0xA2), Some(Modifier::Ctrl));
        assert_eq!(modifier(0x5B), Some(Modifier::Win));
        assert_eq!(modifier(0x41), None);
    }

    #[test]
    fn plain_letters_are_not_standalone() {
        assert!(is_standalone(0x0D));
        assert!(!is_standalone(0x41));
    }
}
