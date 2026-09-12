//! Top-level window enumeration for the window-capture source picker, plus the client
//! rect / monitor resolution the session needs to wire a window target up.
//!
//! Pure Win32 (no windows-capture dependency), so the picker list and the capture
//! target always agree on HWNDs. Filters: visible, has a title, not a tool window, not
//! cloaked (UWP apps suspended on the desktop), and not the capture-ghost windows some
//! frameworks leave behind.

use serde::Serialize;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromWindow, HMONITOR, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClientRect, GetWindowLongW, GetWindowTextLengthW, GetWindowTextW,
    IsWindowVisible, GWL_EXSTYLE, GWL_STYLE,
};

/// One selectable window capture source.
#[derive(Debug, Clone, Serialize)]
pub struct WindowInfo {
    pub hwnd: isize,
    pub title: String,
    /// Client size in physical px.
    pub w: u32,
    pub h: u32,
}

/// `WS_EX_TOOLWINDOW` (floating toolbars) — pointless as a recording subject.
const WS_EX_TOOLWINDOW: i32 = 0x80;
/// A window with no WS_CAPTION has no title bar; almost always chrome-less chrome.
const WS_CAPTION: i32 = 0xC0_0000;

/// The window's display, resolved through the nearest-monitor rule.
#[must_use]
pub fn window_monitor(hwnd: isize) -> crate::session::MonitorInfo {
    let hwnd = HWND(hwnd as _);
    let hmon: HMONITOR = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
    let mut info = MONITORINFO::default();
    info.cbSize = u32::try_from(std::mem::size_of::<MONITORINFO>()).unwrap_or(0);
    if unsafe { GetMonitorInfoW(hmon, &mut info) }.as_bool() {
        return crate::session::MonitorInfo {
            // The device name is the capture-path key; the EXW variant would carry it,
            // but the plain MONITORINFO has none, so reuse the primary naming scheme.
            name: String::new(),
            x: info.rcMonitor.left,
            y: info.rcMonitor.top,
            w: (info.rcMonitor.right - info.rcMonitor.left).max(0) as u32,
            h: (info.rcMonitor.bottom - info.rcMonitor.top).max(0) as u32,
        };
    }
    crate::session::MonitorInfo {
        name: String::new(),
        x: 0,
        y: 0,
        w: 0,
        h: 0,
    }
}

/// The client rect in SCREEN coords (physical px): `(x, y, w, h)`.
///
/// # Errors
/// Fails when the HWND is no longer valid.
pub fn client_screen_rect(hwnd: isize) -> Result<(i32, i32, u32, u32), String> {
    let hwnd = HWND(hwnd as _);
    let mut rc = RECT::default();
    unsafe { GetClientRect(hwnd, &mut rc) }.map_err(|e| format!("GetClientRect failed: {e}"))?;
    let mut pt = windows::Win32::Foundation::POINT {
        x: rc.left,
        y: rc.top,
    };
    // windows-rs 0.62 files ClientToScreen under Graphics::Gdi (metadata quirk).
    unsafe { windows::Win32::Graphics::Gdi::ClientToScreen(hwnd, &mut pt) };
    Ok((
        pt.x,
        pt.y,
        (rc.right - rc.left).max(0) as u32,
        (rc.bottom - rc.top).max(0) as u32,
    ))
}

/// Enumerate selectable windows, roughly z-order (top first).
#[must_use]
pub fn enumerate() -> Vec<WindowInfo> {
    let mut out: Vec<WindowInfo> = Vec::new();
    unsafe {
        let _ = EnumWindows(
            Some(enum_callback),
            windows::Win32::Foundation::LPARAM(&mut out as *mut Vec<WindowInfo> as isize),
        );
    }
    out
}

unsafe extern "system" fn enum_callback(
    hwnd: HWND,
    lparam: windows::Win32::Foundation::LPARAM,
) -> windows::core::BOOL {
    let list = unsafe { &mut *(lparam.0 as *mut Vec<WindowInfo>) };
    if !unsafe { IsWindowVisible(hwnd) }.as_bool() {
        return true.into();
    }
    let ex_style = unsafe { GetWindowLongW(hwnd, GWL_EXSTYLE) };
    if ex_style & WS_EX_TOOLWINDOW != 0 {
        return true.into();
    }
    let style = unsafe { GetWindowLongW(hwnd, GWL_STYLE) };
    if style & WS_CAPTION != WS_CAPTION {
        return true.into();
    }
    // Skip titleless windows early (cheap check before DWM).
    let title_len = unsafe { GetWindowTextLengthW(hwnd) };
    if title_len == 0 {
        return true.into();
    }
    // Skip cloaked UWP windows (suspended apps still "visible" to EnumWindows).
    let mut cloaked: u32 = 0;
    let hr = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            &mut cloaked as *mut u32 as *mut _,
            u32::try_from(std::mem::size_of::<u32>()).unwrap_or(0),
        )
    };
    if hr.is_ok() && cloaked != 0 {
        return true.into();
    }
    let mut buf = [0u16; 256];
    let len = usize::try_from(title_len).unwrap_or(0).min(buf.len());
    unsafe { GetWindowTextW(hwnd, &mut buf) };
    let title = String::from_utf16_lossy(&buf[..len]);
    let mut rc = RECT::default();
    if unsafe { GetClientRect(hwnd, &mut rc) }.is_err() {
        return true.into();
    }
    let w = (rc.right - rc.left).max(0) as u32;
    let h = (rc.bottom - rc.top).max(0) as u32;
    if w < 200 || h < 150 {
        // Tiny windows (launchers, tooltips) make poor demo subjects.
        return true.into();
    }
    list.push(WindowInfo {
        hwnd: hwnd.0 as isize,
        title,
        w,
        h,
    });
    true.into()
}
