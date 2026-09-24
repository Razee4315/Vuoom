//! Win32 helpers for the Tauri windows.
//!
//! Lets the selector/recorder overlays and (during the record flow) the main window opt out
//! of screen capture, so Vuoom's own UI never appears in the recording, and opt back in
//! afterwards, so it appears in the user's own screenshots like any other app.

/// Hide a window from screen capture via `WDA_EXCLUDEFROMCAPTURE` (Windows 10 2004+).
/// The window stays visible on screen but is excluded from WGC / PrintScreen captures.
#[cfg(windows)]
pub fn exclude_from_capture(window: &tauri::WebviewWindow) -> Result<(), String> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        SetWindowDisplayAffinity, WDA_EXCLUDEFROMCAPTURE,
    };
    let hwnd = HWND(window.hwnd().map_err(|e| e.to_string())?.0);
    // SAFETY: standard Win32 call on a realized top-level window handle.
    unsafe { SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE) }.map_err(|e| e.to_string())
}

/// Non-Windows stub (the app is Windows-only, but keeps `cargo check` portable).
#[cfg(not(windows))]
pub fn exclude_from_capture(_window: &tauri::WebviewWindow) -> Result<(), String> {
    Ok(())
}

/// Let screen capture see a window again (`WDA_NONE`): the editor between recordings, so
/// the user's own screenshots and screen shares show Vuoom instead of a black rectangle.
#[cfg(windows)]
pub fn include_in_capture(window: &tauri::WebviewWindow) -> Result<(), String> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{SetWindowDisplayAffinity, WDA_NONE};
    let hwnd = HWND(window.hwnd().map_err(|e| e.to_string())?.0);
    // SAFETY: standard Win32 call on a realized top-level window handle.
    unsafe { SetWindowDisplayAffinity(hwnd, WDA_NONE) }.map_err(|e| e.to_string())
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
