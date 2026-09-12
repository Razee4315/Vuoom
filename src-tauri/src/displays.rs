//! Display enumeration for the record-flow source picker.
//!
//! Uses `EnumDisplayMonitors`/`GetMonitorInfoW` directly (the same GDI source of truth
//! the capture crate's `Monitor::enumerate()` wraps), so the device name handed back to
//! the session matches what `pick_monitor` expects.

use serde::Serialize;
use windows::core::BOOL;
use windows::Win32::Foundation::{LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFOEXW, MONITORINFOF_PRIMARY,
};

/// One selectable capture display.
#[derive(Debug, Clone, Serialize)]
pub struct DisplayInfo {
    /// GDI device name (`\\.\DISPLAY1`), the key the capture path matches on.
    pub name: String,
    /// 1-based index for a friendly label ("Display 1").
    pub index: usize,
    /// Virtual-desktop origin (physical px).
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
    pub primary: bool,
}

/// Enumerate every active display, in GDI enumeration order.
#[must_use]
pub fn enumerate() -> Vec<DisplayInfo> {
    let mut out: Vec<DisplayInfo> = Vec::new();
    unsafe {
        let lparam = LPARAM(&mut out as *mut Vec<DisplayInfo> as isize);
        let _ = EnumDisplayMonitors(None, None, Some(monitor_callback), lparam);
    }
    for (i, d) in out.iter_mut().enumerate() {
        d.index = i + 1;
    }
    out
}

/// Find an enumerated display by its GDI device name.
#[must_use]
pub fn find_by_name(name: &str) -> Option<DisplayInfo> {
    enumerate().into_iter().find(|d| d.name == name)
}

unsafe extern "system" fn monitor_callback(
    _hdc: HDC,
    _hdc_monitor: HDC,
    _rect: *mut RECT,
    hmonitor: HMONITOR,
    lparam: LPARAM,
) -> BOOL {
    let list = unsafe { &mut *(lparam.0 as *mut Vec<DisplayInfo>) };
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = u32::try_from(std::mem::size_of::<MONITORINFOEXW>()).unwrap_or(0);
    if unsafe { GetMonitorInfoW(hmonitor, &info.monitorInfo) }.as_bool() {
        let rc = info.monitorInfo.rcMonitor;
        let device: String = info
            .szDevice
            .iter()
            .take_while(|&&c| c != 0)
            .map(|&c| char::from_u32(u32::from(c)).unwrap_or('\u{FFFD}'))
            .collect();
        list.push(DisplayInfo {
            name: device,
            index: 0,
            x: rc.left,
            y: rc.top,
            w: (rc.right - rc.left).max(0) as u32,
            h: (rc.bottom - rc.top).max(0) as u32,
            primary: info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0,
        });
    }
    true.into()
}
