//! Top-level window enumeration + bounds, for window-targeted capture.
//!
//! `list_windows` enumerates visible, titled, non-tool top-level app windows in Z-order
//! (topmost first) with their **physical-pixel** screen rectangles. Bounds come from
//! `DwmGetWindowAttribute(DWMWA_EXTENDED_FRAME_BOUNDS)` (the true composited frame, which
//! excludes the invisible resize-border padding that `GetWindowRect` includes), falling back
//! to `GetWindowRect` when DWM is unavailable. `find_window_bounds` picks the best match for a
//! case-insensitive title substring. Compile-verified on CI; runtime needs a real desktop.

#[cfg(windows)]
use std::ffi::c_void;

/// A top-level window's title and physical-pixel screen rectangle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowInfo {
    pub title: String,
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

/// Pick the best index in `titles` for a case-insensitive `needle` substring.
///
/// Preference order: (1) an exact case-insensitive title match; otherwise (2) among titles
/// that *contain* the needle, the shortest title (the "tightest" match), breaking ties by
/// lowest index — and since callers pass titles in Z-order, that means the topmost window.
/// Returns `None` when nothing matches. Pure — unit-tested below.
pub(crate) fn best_match_index(titles: &[String], needle: &str) -> Option<usize> {
    let needle_l = needle.to_lowercase();
    if needle_l.is_empty() {
        return None;
    }
    if let Some(i) = titles.iter().position(|t| t.to_lowercase() == needle_l) {
        return Some(i);
    }
    titles
        .iter()
        .enumerate()
        .filter(|(_, t)| t.to_lowercase().contains(&needle_l))
        .min_by_key(|(i, t)| (t.chars().count(), *i))
        .map(|(i, _)| i)
}

/// Enumerate visible, titled, non-tool top-level application windows (topmost first).
#[cfg(windows)]
#[must_use]
pub fn list_windows() -> Vec<WindowInfo> {
    // Leading `::` — the extern crate `windows`, not this `crate::windows` module.
    use ::windows::core::BOOL;
    use ::windows::Win32::Foundation::{HWND, LPARAM, RECT, TRUE};
    use ::windows::Win32::Graphics::Dwm::{
        DwmGetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS,
    };
    use ::windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowLongPtrW, GetWindowRect, GetWindowTextLengthW, GetWindowTextW,
        IsWindowVisible, GWL_EXSTYLE, WS_EX_TOOLWINDOW,
    };

    // Physical-pixel bounds: prefer the DWM extended frame (excludes the invisible resize
    // border), fall back to GetWindowRect.
    unsafe fn window_rect(hwnd: HWND) -> Option<RECT> {
        let mut r = RECT::default();
        let dwm = DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            std::ptr::addr_of_mut!(r).cast::<c_void>(),
            u32::try_from(std::mem::size_of::<RECT>()).unwrap(),
        );
        if dwm.is_ok() {
            return Some(r);
        }
        if GetWindowRect(hwnd, &mut r).is_ok() {
            return Some(r);
        }
        None
    }

    unsafe fn is_cloaked(hwnd: HWND) -> bool {
        let mut cloaked: u32 = 0;
        let ok = DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            std::ptr::addr_of_mut!(cloaked).cast::<c_void>(),
            u32::try_from(std::mem::size_of::<u32>()).unwrap(),
        );
        ok.is_ok() && cloaked != 0
    }

    unsafe fn title_of(hwnd: HWND) -> String {
        let len = GetWindowTextLengthW(hwnd);
        if len <= 0 {
            return String::new();
        }
        let mut buf = vec![0u16; usize::try_from(len).unwrap() + 1];
        let copied = GetWindowTextW(hwnd, &mut buf);
        if copied <= 0 {
            return String::new();
        }
        String::from_utf16_lossy(&buf[..copied as usize])
    }

    unsafe extern "system" fn callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
        // SAFETY: `lparam` carries the &mut Vec<WindowInfo> we passed into EnumWindows.
        let out = &mut *(lparam.0 as *mut Vec<WindowInfo>);

        if !IsWindowVisible(hwnd).as_bool() {
            return TRUE;
        }
        // Skip tool windows (floating palettes, tooltips, etc.).
        let ex_styles = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        if (ex_styles & isize::try_from(WS_EX_TOOLWINDOW.0).unwrap()) != 0 {
            return TRUE;
        }
        // Skip cloaked windows (e.g. suspended/off-screen UWP shells).
        if is_cloaked(hwnd) {
            return TRUE;
        }
        let title = title_of(hwnd);
        if title.is_empty() {
            return TRUE;
        }
        if let Some(r) = window_rect(hwnd) {
            let w = (r.right - r.left).max(0);
            let h = (r.bottom - r.top).max(0);
            if w > 0 && h > 0 {
                out.push(WindowInfo {
                    title,
                    x: r.left,
                    y: r.top,
                    w: w as u32,
                    h: h as u32,
                });
            }
        }
        TRUE
    }

    let mut windows: Vec<WindowInfo> = Vec::new();
    // SAFETY: standard EnumWindows call; the callback only touches the Vec we point at.
    // EnumWindows visits top-level windows in Z-order (topmost first).
    unsafe {
        let _ = EnumWindows(
            Some(callback),
            LPARAM(std::ptr::addr_of_mut!(windows) as isize),
        );
    }
    windows
}

/// Non-Windows stub — the crate is Windows-only but this keeps `cargo check` portable.
#[cfg(not(windows))]
#[must_use]
pub fn list_windows() -> Vec<WindowInfo> {
    Vec::new()
}

/// Find the best-matching visible top-level window for a case-insensitive title substring.
#[must_use]
pub fn find_window_bounds(title_substring: &str) -> Option<WindowInfo> {
    let mut windows = list_windows();
    let titles: Vec<String> = windows.iter().map(|w| w.title.clone()).collect();
    let idx = best_match_index(&titles, title_substring)?;
    Some(windows.swap_remove(idx))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn titles(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn exact_case_insensitive_match_wins() {
        let t = titles(&["Some Notepad Thing", "Notepad", "notepad extra"]);
        assert_eq!(best_match_index(&t, "notepad"), Some(1));
    }

    #[test]
    fn substring_picks_shortest_then_topmost() {
        // No exact match; all contain "code". Shortest title is "VS Code" (idx 2),
        // but "Code" (idx 3) is shorter still.
        let t = titles(&[
            "project - Visual Studio Code",
            "readme - Visual Studio Code",
            "VS Code",
            "Code",
        ]);
        assert_eq!(best_match_index(&t, "code"), Some(3));
    }

    #[test]
    fn substring_ties_break_to_topmost() {
        // Two equally-short matches; the earlier (topmost) index wins.
        let t = titles(&["My App", "My App"]);
        assert_eq!(best_match_index(&t, "app"), Some(0));
    }

    #[test]
    fn no_match_returns_none() {
        let t = titles(&["Firefox", "Explorer"]);
        assert_eq!(best_match_index(&t, "chrome"), None);
    }

    #[test]
    fn empty_needle_returns_none() {
        let t = titles(&["Firefox"]);
        assert_eq!(best_match_index(&t, ""), None);
    }
}
