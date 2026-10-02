//! Which pointer the system is showing: the arrow, the text beam over a text box, the hand
//! over a link, a resize arrow on a window's edge...
//!
//! A take recorded with the pointer hidden gets a clean one drawn from the input log. Left
//! as an arrow everywhere, it looks fake the moment it crosses a text field; so the hook
//! thread notes each time the real pointer changes shape, and the drawn one follows.
//!
//! Windows identifies the pointer by handle, and the standard pointers are shared system
//! objects: the handle an app shows is the same one `LoadCursorW` returns here. A pointer
//! an app drew itself matches none of them and is reported as [`CursorKind::Other`].

use crate::event::CursorKind;
use std::cell::Cell;
use windows::Win32::UI::WindowsAndMessaging as wm;

thread_local! {
    /// The standard pointers' handles (as integers) and what each one is.
    static STANDARD: Vec<(isize, CursorKind)> = standard_cursors();
    /// The handle last seen (0 = none yet).
    static LAST: Cell<isize> = const { Cell::new(0) };
}

fn standard_cursors() -> Vec<(isize, CursorKind)> {
    let table = [
        (wm::IDC_ARROW, CursorKind::Arrow),
        (wm::IDC_APPSTARTING, CursorKind::Arrow),
        (wm::IDC_IBEAM, CursorKind::Text),
        (wm::IDC_HAND, CursorKind::Hand),
        (wm::IDC_CROSS, CursorKind::Cross),
        (wm::IDC_SIZEWE, CursorKind::ResizeH),
        (wm::IDC_SIZENS, CursorKind::ResizeV),
        (wm::IDC_SIZENWSE, CursorKind::ResizeNwse),
        (wm::IDC_SIZENESW, CursorKind::ResizeNesw),
        (wm::IDC_SIZEALL, CursorKind::Move),
    ];
    let mut known = Vec::with_capacity(table.len());
    for (id, kind) in table {
        // SAFETY: loads a shared system pointer; nothing to free.
        if let Ok(cursor) = unsafe { wm::LoadCursorW(None, id) } {
            known.push((cursor.0 as isize, kind));
        }
    }
    known
}

/// The kind of pointer now showing, if it changed since the last call on this thread.
/// A hidden pointer (an app hides it while you type) changes nothing.
pub(crate) fn changed() -> Option<CursorKind> {
    let mut info = wm::CURSORINFO {
        cbSize: std::mem::size_of::<wm::CURSORINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: `info` is a live CURSORINFO with its size set.
    unsafe { wm::GetCursorInfo(&mut info) }.ok()?;
    let handle = info.hCursor.0 as isize;
    if handle == 0 || LAST.with(|last| last.replace(handle)) == handle {
        return None;
    }
    let kind = STANDARD.with(|known| {
        let found = known.iter().find(|(h, _)| *h == handle);
        found.map_or(CursorKind::Other, |(_, kind)| *kind)
    });
    Some(kind)
}
