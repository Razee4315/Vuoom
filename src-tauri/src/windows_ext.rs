//! Win32 helpers for the Tauri windows.
//!
//! Lets the selector/recorder overlays and (during capture) the main window opt out of
//! Windows Graphics Capture, so Vuoom's own UI never appears in the recording.

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

/// Put a file on the clipboard as `CF_HDROP`, so pasting into Slack / Discord / a GitHub
/// comment uploads the actual (animated) file. Windows has no animated-GIF clipboard
/// format — copying the *file* is what every real tool does. See `docs/06-Export.md`.
#[cfg(windows)]
pub fn copy_file_to_clipboard(path: &str) -> Result<(), String> {
    use std::mem::size_of;
    use windows::Win32::Foundation::{GlobalFree, HANDLE};
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
    };
    use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
    use windows::Win32::System::Ole::CF_HDROP;
    use windows::Win32::UI::Shell::DROPFILES;

    // CF_HDROP payload: a DROPFILES header followed by a double-NUL-terminated UTF-16 list.
    let wide: Vec<u16> = path.encode_utf16().chain([0u16, 0u16]).collect();
    let header = size_of::<DROPFILES>();
    let total = header + wide.len() * 2;

    // SAFETY: standard CF_HDROP construction — allocate moveable global memory, fill the
    // DROPFILES header + path list, hand ownership to the clipboard on success.
    unsafe {
        let hglobal = GlobalAlloc(GMEM_MOVEABLE, total).map_err(|e| e.to_string())?;
        let ptr = GlobalLock(hglobal);
        if ptr.is_null() {
            let _ = GlobalFree(Some(hglobal));
            return Err("GlobalLock failed".into());
        }
        let drop_files = ptr.cast::<DROPFILES>();
        (*drop_files) = DROPFILES {
            pFiles: header as u32,
            fWide: true.into(),
            ..Default::default()
        };
        std::ptr::copy_nonoverlapping(
            wide.as_ptr(),
            ptr.cast::<u8>().add(header).cast::<u16>(),
            wide.len(),
        );
        let _ = GlobalUnlock(hglobal);

        if let Err(e) = OpenClipboard(None) {
            let _ = GlobalFree(Some(hglobal));
            return Err(e.to_string());
        }
        let result = EmptyClipboard()
            .and_then(|()| SetClipboardData(u32::from(CF_HDROP.0), Some(HANDLE(hglobal.0))));
        let _ = CloseClipboard();
        match result {
            Ok(_) => Ok(()), // the clipboard now owns the memory
            Err(e) => {
                let _ = GlobalFree(Some(hglobal));
                Err(e.to_string())
            }
        }
    }
}

/// Non-Windows stub.
#[cfg(not(windows))]
pub fn copy_file_to_clipboard(_path: &str) -> Result<(), String> {
    Err("clipboard file copy is Windows-only".into())
}
