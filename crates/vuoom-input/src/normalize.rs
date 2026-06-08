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
        assert!(ev.is_zoom_trigger());
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
}
