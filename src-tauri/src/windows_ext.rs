//! Win32 helpers for the Tauri windows.
//!
//! Lets the selector/recorder overlays and (during the record flow) the main window opt out
//! of screen capture, so Vuoom's own UI never appears in the recording, and opt back in
//! afterwards, so it appears in the user's own screenshots like any other app.

/// Keeps an excluded window layered. tao rewrites a window's whole extended style from its
/// own flags whenever one changes (fullscreen, always on top, resizable...), which would
/// drop `WS_EX_LAYERED`; this window subclass puts it back in every `WM_STYLECHANGING`
/// while the window is excluded from capture.
#[cfg(windows)]
mod layered_guard {
    use std::sync::atomic::{AtomicBool, Ordering};
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
    use windows::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, STYLESTRUCT, WM_STYLECHANGING, WS_EX_LAYERED,
    };

    /// Our subclass id on the main window (the drag wall uses 0x7600_0001).
    const SUBCLASS_ID: usize = 0x7600_0002;

    /// Whether the guard is keeping the window layered.
    pub static ON: AtomicBool = AtomicBool::new(false);

    unsafe extern "system" fn subclass_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _id: usize,
        _data: usize,
    ) -> LRESULT {
        let ex_style = wparam.0 as i32 == GWL_EXSTYLE.0;
        if msg == WM_STYLECHANGING && ex_style && ON.load(Ordering::Relaxed) {
            // SAFETY: for WM_STYLECHANGING, lParam points to the STYLESTRUCT being applied.
            let change = unsafe { &mut *(lparam.0 as *mut STYLESTRUCT) };
            change.styleNew |= WS_EX_LAYERED.0;
        }
        unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
    }

    /// Subclass `hwnd`. Only works on the window's own (UI) thread: `false` elsewhere.
    pub fn install(hwnd: HWND) -> bool {
        // SAFETY: standard subclass install on a realized top-level window.
        unsafe { SetWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID, 0) }.as_bool()
    }

    /// Drop the subclass (UI thread only; `false` elsewhere).
    pub fn remove(hwnd: HWND) -> bool {
        // SAFETY: removing our own subclass by matching proc + id.
        unsafe { RemoveWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID) }.as_bool()
    }
}

/// Hide a window from screen capture via `WDA_EXCLUDEFROMCAPTURE` (Windows 10 2004+).
/// The window stays visible on screen but is excluded from Desktop Duplication, WGC and
/// PrintScreen captures. Safe to call again: each call re-arms the exclusion.
///
/// On Windows 10 an excluded window that is hidden and shown again, or whose styles are
/// rewritten (tao does that on fullscreen, always-on-top and resizable changes), is captured
/// as a solid BLACK box instead of being left out, while it still reports
/// `WDA_EXCLUDEFROMCAPTURE`. A layered window doesn't hit that (the fix Electron ships for
/// `setContentProtection`), so the window is made layered (fully opaque: it looks the same)
/// and kept layered by [`layered_guard`]. A black-out that already set in is undone only by
/// clearing the affinity and setting it again (setting the same value again does nothing),
/// hence `WDA_NONE` first. All of this was reproduced and verified on Windows 10 22H2 with a
/// WebView2 window, in Desktop Duplication and GDI captures.
#[cfg(windows)]
pub fn exclude_from_capture(window: &tauri::WebviewWindow) -> Result<(), String> {
    use std::sync::atomic::Ordering;
    use windows::Win32::Foundation::{COLORREF, HWND};
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetLayeredWindowAttributes, SetWindowDisplayAffinity, SetWindowLongPtrW,
        GWL_EXSTYLE, LWA_ALPHA, WDA_EXCLUDEFROMCAPTURE, WDA_NONE, WS_EX_LAYERED,
    };
    let hwnd = HWND(window.hwnd().map_err(|e| e.to_string())?.0);
    layered_guard::ON.store(true, Ordering::Relaxed);
    if !layered_guard::install(hwnd) {
        // Not on the UI thread: subclass from there (a raw handle crosses threads).
        let raw = hwnd.0 as isize;
        let _ = window.run_on_main_thread(move || {
            layered_guard::install(HWND(raw as _));
        });
    }
    // SAFETY: standard Win32 calls on a realized top-level window handle of this process.
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let _ = SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex | WS_EX_LAYERED.0 as isize);
        let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA);
        let _ = SetWindowDisplayAffinity(hwnd, WDA_NONE);
        SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE)
    }
    .map_err(|e| e.to_string())
}

/// Non-Windows stub (the app is Windows-only, but keeps `cargo check` portable).
#[cfg(not(windows))]
pub fn exclude_from_capture(_window: &tauri::WebviewWindow) -> Result<(), String> {
    Ok(())
}

/// Let screen capture see a window again (`WDA_NONE`): the editor between recordings, so
/// the user's own screenshots and screen shares show Vuoom instead of a black rectangle.
/// The layered style and its guard from [`exclude_from_capture`] are removed again.
#[cfg(windows)]
pub fn include_in_capture(window: &tauri::WebviewWindow) -> Result<(), String> {
    use std::sync::atomic::Ordering;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowDisplayAffinity, SetWindowLongPtrW, GWL_EXSTYLE, WDA_NONE,
        WS_EX_LAYERED,
    };
    let hwnd = HWND(window.hwnd().map_err(|e| e.to_string())?.0);
    layered_guard::ON.store(false, Ordering::Relaxed);
    if !layered_guard::remove(hwnd) {
        let raw = hwnd.0 as isize;
        let _ = window.run_on_main_thread(move || {
            layered_guard::remove(HWND(raw as _));
        });
    }
    // SAFETY: standard Win32 calls on a realized top-level window handle of this process.
    unsafe {
        let set = SetWindowDisplayAffinity(hwnd, WDA_NONE);
        set.map_err(|e| e.to_string())?;
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let _ = SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex & !(WS_EX_LAYERED.0 as isize));
    }
    Ok(())
}

/// Non-Windows stub.
#[cfg(not(windows))]
pub fn include_in_capture(_window: &tauri::WebviewWindow) -> Result<(), String> {
    Ok(())
}

/// Put a file on the clipboard as `CF_HDROP`, so pasting into Slack / Discord / a GitHub
/// comment uploads the actual (animated) file. Windows has no animated-GIF clipboard
/// format, copying the *file* is what every real tool does. See `docs/06-Export.md`.
///
/// `clipboard-win` builds the `DROPFILES` payload and manages the clipboard open/close +
/// global-memory ownership, so this stays safe instead of hand-rolled `unsafe`.
#[cfg(windows)]
pub fn copy_file_to_clipboard(path: &str) -> Result<(), String> {
    use clipboard_win::{options, raw, Clipboard};

    // `Setter<[T]>` is only implemented for the unsized slice, which the generic
    // `set_clipboard` can't take by value, so open the clipboard explicitly and use the
    // raw file-list writer. `DoClear` empties the clipboard first, matching the old
    // EmptyClipboard behavior.
    let _clip = Clipboard::new_attempts(10).map_err(|e| e.to_string())?;
    raw::set_file_list_with(&[path], options::DoClear).map_err(|e| e.to_string())
}

/// Non-Windows stub.
#[cfg(not(windows))]
pub fn copy_file_to_clipboard(_path: &str) -> Result<(), String> {
    Err("clipboard file copy is Windows-only".into())
}
