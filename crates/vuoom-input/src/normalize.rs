//! Bridge raw physical events into normalized, frame-relative zoom events.
//!
//! Pure and unit-tested: given a captured region (physical px) and the recording's QPC
//! epoch, convert each [`RawEvent`] into a [`vuoom_zoom::InputEvent`] in `0.0..=1.0`
//! space with a time relative to recording start.

use crate::event::{MouseButton, RawEvent, RawEventKind};
use glam::DVec2;
use vuoom_zoom::{InputEvent, MouseButton as ZButton};

/// The captured region in physical virtual-desktop pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaptureRegion {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

fn map_button(b: MouseButton) -> ZButton {
    match b {
        MouseButton::Left => ZButton::Left,
        MouseButton::Right => ZButton::Right,
        MouseButton::Middle => ZButton::Middle,
        MouseButton::X1 | MouseButton::X2 => ZButton::Other,
    }
}

/// Convert a raw event into a normalized zoom [`InputEvent`].
///
/// Returns `None` for events that do not drive auto-zoom (button-up, key-up).
#[must_use]
pub fn normalize(
    raw: &RawEvent,
    region: &CaptureRegion,
    start_qpc: i64,
    freq: i64,
) -> Option<InputEvent> {
    let freq = freq.max(1);
    let t = (raw.qpc - start_qpc) as f64 / freq as f64;
    let w = f64::from(region.w.max(1));
    let h = f64::from(region.h.max(1));
    let pos = DVec2::new(
        f64::from(raw.x - region.x) / w,
        f64::from(raw.y - region.y) / h,
    );
    match raw.kind {
        RawEventKind::Move => Some(InputEvent::Move { t, pos }),
        RawEventKind::ButtonDown(b) => Some(InputEvent::Click {
            t,
            pos,
            button: map_button(b),
        }),
        RawEventKind::Scroll(d) => Some(InputEvent::Scroll {
            t,
            pos,
            delta: f64::from(d),
        }),
        RawEventKind::KeyDown(_) => Some(InputEvent::KeyType { t }),
        RawEventKind::ButtonUp(_) | RawEventKind::KeyUp(_) => None,
    }
}

/// The manual zoom hotkey: **Ctrl + Shift + Z**, pressed at the cursor while recording.
mod zoom_hotkey {
    pub const VK_Z: u16 = 0x5A;
    /// `VK_CONTROL`, `VK_LCONTROL`, `VK_RCONTROL`.
    pub fn is_ctrl(vk: u16) -> bool {
        matches!(vk, 0x11 | 0xA2 | 0xA3)
    }
    /// `VK_SHIFT`, `VK_LSHIFT`, `VK_RSHIFT`.
    pub fn is_shift(vk: u16) -> bool {
        matches!(vk, 0x10 | 0xA0 | 0xA1)
    }
}

/// Scan a chronological raw event log for the manual zoom hotkey (**Ctrl+Shift+Z**) and
/// emit a [`InputEvent::ZoomMark`] at the cursor's normalized position for each *press*
/// (auto-repeat while held is ignored).
///
/// The cursor position is taken from the most recent mouse event before the keystroke,
/// so the zoom centers where the user is pointing. Pure and unit-tested.
#[must_use]
pub fn zoom_marks(
    raw: &[RawEvent],
    region: &CaptureRegion,
    start_qpc: i64,
    freq: i64,
) -> Vec<InputEvent> {
    let freq = freq.max(1);
    let w = f64::from(region.w.max(1));
    let h = f64::from(region.h.max(1));

    let mut ctrl = false;
    let mut shift = false;
    let mut z_held = false;
    // Default to the region center until we see a real cursor position.
    let mut cx = region.x + region.w / 2;
    let mut cy = region.y + region.h / 2;
    let mut marks = Vec::new();

    for e in raw {
        match e.kind {
            RawEventKind::Move
            | RawEventKind::ButtonDown(_)
            | RawEventKind::ButtonUp(_)
            | RawEventKind::Scroll(_) => {
                cx = e.x;
                cy = e.y;
            }
            RawEventKind::KeyDown(vk) if zoom_hotkey::is_ctrl(vk) => ctrl = true,
            RawEventKind::KeyDown(vk) if zoom_hotkey::is_shift(vk) => shift = true,
            RawEventKind::KeyDown(vk) if vk == zoom_hotkey::VK_Z => {
                if ctrl && shift && !z_held {
                    let t = (e.qpc - start_qpc) as f64 / freq as f64;
                    let pos = DVec2::new(
                        (f64::from(cx - region.x) / w).clamp(0.0, 1.0),
                        (f64::from(cy - region.y) / h).clamp(0.0, 1.0),
                    );
                    marks.push(InputEvent::ZoomMark { t, pos });
                }
                z_held = true;
            }
            RawEventKind::KeyUp(vk) if zoom_hotkey::is_ctrl(vk) => ctrl = false,
            RawEventKind::KeyUp(vk) if zoom_hotkey::is_shift(vk) => shift = false,
            RawEventKind::KeyUp(vk) if vk == zoom_hotkey::VK_Z => z_held = false,
            RawEventKind::KeyDown(_) | RawEventKind::KeyUp(_) => {}
        }
    }
    marks
}

#[cfg(test)]
mod tests {
    use super::*;

    fn region() -> CaptureRegion {
        CaptureRegion {
            x: 100,
            y: 200,
            w: 1000,
            h: 500,
        }
    }

    #[test]
    fn center_move_maps_to_half() {
        let raw = RawEvent {
            qpc: 1000,
            x: 100 + 500,
            y: 200 + 250,
            kind: RawEventKind::Move,
        };
        let ev = normalize(&raw, &region(), 0, 1000).unwrap();
        match ev {
            InputEvent::Move { t, pos } => {
                assert!((t - 1.0).abs() < 1e-9);
                assert!((pos.x - 0.5).abs() < 1e-9);
                assert!((pos.y - 0.5).abs() < 1e-9);
            }
            _ => panic!("expected Move"),
        }
    }

    #[test]
    fn click_maps_button_and_is_a_trigger() {
        let raw = RawEvent {
            qpc: 0,
            x: 100,
            y: 200,
            kind: RawEventKind::ButtonDown(MouseButton::Left),
        };
        let ev = normalize(&raw, &region(), 0, 1000).unwrap();
        assert!(matches!(ev, InputEvent::Click { .. }));
        assert!(ev.is_click_trigger());
    }

    #[test]
    fn button_up_and_key_up_are_dropped() {
        let up = RawEvent {
            qpc: 0,
            x: 0,
            y: 0,
            kind: RawEventKind::ButtonUp(MouseButton::Left),
        };
        assert!(normalize(&up, &region(), 0, 1000).is_none());
    }

    fn key(qpc: i64, kind: RawEventKind) -> RawEvent {
        RawEvent {
            qpc,
            x: 0,
            y: 0,
            kind,
        }
    }

    #[test]
    fn ctrl_shift_z_makes_a_zoom_mark_at_the_cursor() {
        // Cursor lands at the region center, then Ctrl+Shift+Z is pressed.
        let raw = [
            RawEvent {
                qpc: 500,
                x: 100 + 500,
                y: 200 + 250,
                kind: RawEventKind::Move,
            },
            key(1000, RawEventKind::KeyDown(0x11)), // Ctrl
            key(1010, RawEventKind::KeyDown(0x10)), // Shift
            key(1020, RawEventKind::KeyDown(0x5A)), // Z
        ];
        let marks = zoom_marks(&raw, &region(), 0, 1000);
        assert_eq!(marks.len(), 1);
        match marks[0] {
            vuoom_zoom::InputEvent::ZoomMark { t, pos } => {
                assert!((t - 1.02).abs() < 1e-9);
                assert!((pos.x - 0.5).abs() < 1e-9 && (pos.y - 0.5).abs() < 1e-9);
            }
            _ => panic!("expected ZoomMark"),
        }
    }

    #[test]
    fn z_alone_or_without_both_modifiers_does_nothing() {
        let raw = [
            key(10, RawEventKind::KeyDown(0x5A)), // Z, no modifiers
            key(20, RawEventKind::KeyDown(0x11)), // Ctrl only
            key(30, RawEventKind::KeyDown(0x5A)), // Z with Ctrl only
        ];
        assert!(zoom_marks(&raw, &region(), 0, 1000).is_empty());
    }

    #[test]
    fn auto_repeat_while_held_marks_only_once() {
        let raw = [
            key(10, RawEventKind::KeyDown(0x11)),
            key(20, RawEventKind::KeyDown(0x10)),
            key(30, RawEventKind::KeyDown(0x5A)), // press
            key(40, RawEventKind::KeyDown(0x5A)), // auto-repeat
            key(50, RawEventKind::KeyDown(0x5A)), // auto-repeat
        ];
        assert_eq!(zoom_marks(&raw, &region(), 0, 1000).len(), 1);
    }
}
