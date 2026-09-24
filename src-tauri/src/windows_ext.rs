//! Win32 helpers for the Tauri windows.
//!
//! Lets the selector/recorder overlays and (during the record flow) the main window opt out
//! of screen capture, so Vuoom's own UI never appears in the recording, and opt back in
//! afterwards, so it appears in the user's own screenshots like any other app.

/// Hide a window from screen capture via `WDA_EXCLUDEFROMCAPTURE` (Windows 10 2004+).
/// The window stays visible on screen but is excluded from Desktop Duplication, WGC and
/// PrintScreen captures.
///
/// The window is also made layered (fully opaque, so it looks the same). On Windows 10 an
/// excluded window that is hidden and shown again is captured as a solid BLACK box instead of
/// being left out, although it still reports `WDA_EXCLUDEFROMCAPTURE`; the record flow hides
/// the window for the region picker's screenshot, which is exactly that. A layered window
/// doesn't hit the bug (the same fix Electron ships for `setContentProtection`). Verified on
/// Windows 10 22H2 with a WebView2 window: hide/show after excluding records black, with
/// `WS_EX_LAYERED` the desktop behind shows.
#[cfg(windows)]
pub fn exclude_from_capture(window: &tauri::WebviewWindow) -> Result<(), String> {
    use windows::Win32::Foundation::{COLORREF, HWND};
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetLayeredWindowAttributes, SetWindowDisplayAffinity, SetWindowLongPtrW,
        GWL_EXSTYLE, LWA_ALPHA, WDA_EXCLUDEFROMCAPTURE, WS_EX_LAYERED,
    };
    let hwnd = HWND(window.hwnd().map_err(|e| e.to_string())?.0);
    // SAFETY: standard Win32 calls on a realized top-level window handle of this process.
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let _ = SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex | WS_EX_LAYERED.0 as isize);
        let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA);
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
/// The layered style [`exclude_from_capture`] added is removed again.
#[cfg(windows)]
pub fn include_in_capture(window: &tauri::WebviewWindow) -> Result<(), String> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowDisplayAffinity, SetWindowLongPtrW, GWL_EXSTYLE, WDA_NONE,
        WS_EX_LAYERED,
    };
    let hwnd = HWND(window.hwnd().map_err(|e| e.to_string())?.0);
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
