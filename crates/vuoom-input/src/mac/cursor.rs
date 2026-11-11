//! Which pointer macOS is showing: arrow, I-beam, pointing hand, resize arrows...
//!
//! Windows hands out shared handles for its standard pointers; macOS has no such thing, but
//! the system cursor reports its hotspot and image size, and each standard cursor has its
//! own pair. They are read once from `NSCursor`'s standard set and the current one is
//! matched against them. A cursor an app drew itself matches none and is
//! [`CursorKind::Other`].

use crate::event::CursorKind;
use objc2::rc::Retained;
use objc2_app_kit::NSCursor;
use std::cell::{Cell, RefCell};

/// A cursor's identity for matching: hotspot and image size, rounded to quarter points.
type Key = (i32, i32, i32, i32);

fn key(c: &NSCursor) -> Key {
    let hot = c.hotSpot();
    let size = c.image().size();
    let q = |v: f64| (v * 4.0).round() as i32;
    (q(hot.x), q(hot.y), q(size.width), q(size.height))
}

#[allow(deprecated)] // the resize cursors keep working; their replacements are 15+ only
fn standard() -> Vec<(Key, CursorKind)> {
    let table: [(Retained<NSCursor>, CursorKind); 10] = [
        (NSCursor::arrowCursor(), CursorKind::Arrow),
        (NSCursor::IBeamCursor(), CursorKind::Text),
        (NSCursor::pointingHandCursor(), CursorKind::Hand),
        (NSCursor::crosshairCursor(), CursorKind::Cross),
        (NSCursor::resizeLeftRightCursor(), CursorKind::ResizeH),
        (NSCursor::resizeLeftCursor(), CursorKind::ResizeH),
        (NSCursor::resizeRightCursor(), CursorKind::ResizeH),
        (NSCursor::resizeUpDownCursor(), CursorKind::ResizeV),
        (NSCursor::openHandCursor(), CursorKind::Move),
        (NSCursor::closedHandCursor(), CursorKind::Move),
    ];
    let mut out: Vec<(Key, CursorKind)> = Vec::new();
    for (cursor, kind) in &table {
        let k = key(cursor);
        // The first kind listed wins a tie (the arrow before anything that shares its key).
        if !out.iter().any(|(seen, _)| *seen == k) {
            out.push((k, *kind));
        }
    }
    out
}

thread_local! {
    static KNOWN: RefCell<Option<Vec<(Key, CursorKind)>>> = const { RefCell::new(None) };
    static LAST: Cell<Option<Key>> = const { Cell::new(None) };
}

/// The kind of pointer now showing, if it changed since the last call on this thread.
#[allow(deprecated)] // still answers on current macOS; `None` (no shapes) if it ever stops
pub(crate) fn changed() -> Option<CursorKind> {
    let current = NSCursor::currentSystemCursor()?;
    let k = key(&current);
    if LAST.with(|last| last.replace(Some(k))) == Some(k) {
        return None;
    }
    let kind = KNOWN.with(|known| {
        let mut known = known.borrow_mut();
        let list = known.get_or_insert_with(standard);
        list.iter()
            .find(|(seen, _)| *seen == k)
            .map_or(CursorKind::Other, |(_, kind)| *kind)
    });
    Some(kind)
}
