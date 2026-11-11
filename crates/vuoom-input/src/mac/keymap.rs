//! macOS key codes (`kVK_*`, layout independent, from `HIToolbox/Events.h`) ↔ Win32
//! virtual-key codes.
//!
//! The rest of Vuoom speaks Win32 VK codes: the keystroke overlay's names, the hotkey
//! chords, the zoom-mark scan. Translating at the event tap keeps all of that unchanged.

/// `(macOS key code, Win32 VK)` for every key Vuoom names or can bind.
const TABLE: &[(u16, u16)] = &[
    // Letters (ANSI positions).
    (0x00, 0x41), // A
    (0x0B, 0x42), // B
    (0x08, 0x43), // C
    (0x02, 0x44), // D
    (0x0E, 0x45), // E
    (0x03, 0x46), // F
    (0x05, 0x47), // G
    (0x04, 0x48), // H
    (0x22, 0x49), // I
    (0x26, 0x4A), // J
    (0x28, 0x4B), // K
    (0x25, 0x4C), // L
    (0x2E, 0x4D), // M
    (0x2D, 0x4E), // N
    (0x1F, 0x4F), // O
    (0x23, 0x50), // P
    (0x0C, 0x51), // Q
    (0x0F, 0x52), // R
    (0x01, 0x53), // S
    (0x11, 0x54), // T
    (0x20, 0x55), // U
    (0x09, 0x56), // V
    (0x0D, 0x57), // W
    (0x07, 0x58), // X
    (0x10, 0x59), // Y
    (0x06, 0x5A), // Z
    // Digits.
    (0x1D, 0x30),
    (0x12, 0x31),
    (0x13, 0x32),
    (0x14, 0x33),
    (0x15, 0x34),
    (0x17, 0x35),
    (0x16, 0x36),
    (0x1A, 0x37),
    (0x1C, 0x38),
    (0x19, 0x39),
    // Function keys.
    (0x7A, 0x70), // F1
    (0x78, 0x71),
    (0x63, 0x72),
    (0x76, 0x73),
    (0x60, 0x74),
    (0x61, 0x75),
    (0x62, 0x76),
    (0x64, 0x77),
    (0x65, 0x78),
    (0x6D, 0x79),
    (0x67, 0x7A),
    (0x6F, 0x7B), // F12
    // Editing and navigation.
    (0x24, 0x0D), // Return → Enter
    (0x4C, 0x0D), // keypad Enter
    (0x30, 0x09), // Tab
    (0x31, 0x20), // Space
    (0x33, 0x08), // Delete (backspace)
    (0x75, 0x2E), // Forward delete → Del
    (0x35, 0x1B), // Escape
    (0x72, 0x2D), // Help → Ins
    (0x73, 0x24), // Home
    (0x77, 0x23), // End
    (0x74, 0x21), // Page up
    (0x79, 0x22), // Page down
    (0x7B, 0x25), // ←
    (0x7E, 0x26), // ↑
    (0x7C, 0x27), // →
    (0x7D, 0x28), // ↓
    // Punctuation (US positions).
    (0x18, 0xBB), // = (the + key)
    (0x2B, 0xBC), // ,
    (0x1B, 0xBD), // -
    (0x2F, 0xBE), // .
    (0x2C, 0xBF), // /
    (0x32, 0xC0), // `
    (0x29, 0xBA), // ;
    (0x21, 0xDB), // [
    (0x2A, 0xDC), // \
    (0x1E, 0xDD), // ]
    (0x27, 0xDE), // '
    // Modifiers, as the left/right VKs a low-level Windows hook reports.
    (0x38, 0xA0), // Shift
    (0x3C, 0xA1), // right Shift
    (0x3B, 0xA2), // Control
    (0x3E, 0xA3), // right Control
    (0x3A, 0xA4), // Option → Alt
    (0x3D, 0xA5), // right Option
    (0x37, 0x5B), // Command → left Windows key
    (0x36, 0x5C), // right Command
    (0x39, 0x14), // Caps Lock
];

/// The Win32 VK code for a macOS key code, if Vuoom knows the key.
#[must_use]
pub fn mac_to_vk(code: u16) -> Option<u16> {
    TABLE.iter().find(|(m, _)| *m == code).map(|(_, vk)| *vk)
}

/// The macOS key code for a Win32 VK code (the first one, for VKs two keys share).
#[must_use]
pub fn vk_to_mac(vk: u16) -> Option<u16> {
    // The generic modifier VKs a chord check may ask about map to the left key.
    let vk = match vk {
        0x10 => 0xA0,
        0x11 => 0xA2,
        0x12 => 0xA4,
        other => other,
    };
    TABLE.iter().find(|(_, v)| *v == vk).map(|(m, _)| *m)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_digits_and_keys_translate_both_ways() {
        assert_eq!(mac_to_vk(0x06), Some(0x5A)); // Z
        assert_eq!(mac_to_vk(0x07), Some(0x58)); // X
        assert_eq!(mac_to_vk(0x1D), Some(0x30)); // 0
        assert_eq!(mac_to_vk(0x6F), Some(0x7B)); // F12
        assert_eq!(mac_to_vk(0x37), Some(0x5B)); // Command
        assert_eq!(mac_to_vk(0x24), Some(0x0D)); // Return
        assert_eq!(mac_to_vk(0x0A), None); // ISO § key: unnamed
        assert_eq!(vk_to_mac(0x5A), Some(0x06));
        assert_eq!(vk_to_mac(0x11), Some(0x3B));
        assert_eq!(vk_to_mac(0x0D), Some(0x24));
    }

    #[test]
    fn every_letter_and_digit_is_mapped_once() {
        for vk in (0x41..=0x5A).chain(0x30..=0x39).chain(0x70..=0x7B) {
            let n = TABLE.iter().filter(|(_, v)| *v == vk).count();
            assert_eq!(n, 1, "vk {vk:#x}");
        }
        let mut codes: Vec<u16> = TABLE.iter().map(|(m, _)| *m).collect();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), TABLE.len(), "a macOS key code is listed twice");
    }
}
