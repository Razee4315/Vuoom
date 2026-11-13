//! Top-level window enumeration for the window-capture picker (macOS).
//!
//! ScreenCaptureKit lists the windows (the same source the capture itself resolves ids
//! against, so the two always agree). A window's id is its `CGWindowID`, carried in the
//! `hwnd` field the Windows build uses for HWNDs. Geometry is in the pixel space input
//! events use (`vuoom_input::mac`).

use serde::Serialize;
use vuoom_capture::CropRegion;

/// One selectable window capture source.
#[derive(Debug, Clone, Serialize)]
pub struct WindowInfo {
    /// The `CGWindowID`.
    pub hwnd: isize,
    pub title: String,
    /// The window's top-left corner in pixels (see the module docs).
    pub x: i32,
    pub y: i32,
    /// Size in pixels.
    pub w: u32,
    pub h: u32,
}

fn to_info(w: &vuoom_capture::ShareableWindow) -> WindowInfo {
    let (x, y, pw, ph) = pixel_rect(w);
    let title = if w.title.contains(&w.app) {
        w.title.clone()
    } else {
        format!("{}: {}", w.app, w.title)
    };
    WindowInfo {
        hwnd: w.id as isize,
        title,
        x,
        y,
        w: pw,
        h: ph,
    }
}

/// A window's frame (points) in pixels, scaled by the display it sits on.
fn pixel_rect(w: &vuoom_capture::ShareableWindow) -> (i32, i32, u32, u32) {
    let (x, y) = vuoom_input::mac::to_pixels(w.x, w.y);
    let (cx, cy) = (w.x + w.w / 2.0, w.y + w.h / 2.0);
    let scale = vuoom_input::mac::displays()
        .into_iter()
        .find(|d| cx >= d.x && cx < d.x + d.w && cy >= d.y && cy < d.y + d.h)
        .map_or(1.0, |d| d.scale);
    (
        x,
        y,
        (w.w * scale).round() as u32,
        (w.h * scale).round() as u32,
    )
}

fn find(id: isize) -> Option<vuoom_capture::ShareableWindow> {
    vuoom_capture::shareable_windows()
        .ok()?
        .into_iter()
        .find(|w| w.id as isize == id)
}

/// What to crop a window capture's frames to. macOS window frames are the window itself
/// (no separate client area), so it is `region` or the whole window.
#[must_use]
pub fn client_crop(_hwnd: isize, w: u32, h: u32, region: Option<CropRegion>) -> CropRegion {
    region.unwrap_or(CropRegion { x: 0, y: 0, w, h })
}

/// The display the window is mostly on.
#[must_use]
pub fn window_monitor(hwnd: isize) -> crate::session::MonitorInfo {
    let center = find(hwnd).map(|w| (w.x + w.w / 2.0, w.y + w.h / 2.0));
    let list = vuoom_input::mac::displays();
    let d = center
        .and_then(|(cx, cy)| {
            list.iter()
                .find(|d| cx >= d.x && cx < d.x + d.w && cy >= d.y && cy < d.y + d.h)
        })
        .or_else(|| list.iter().find(|d| d.primary));
    match d {
        Some(d) => {
            let (x, y) = d.pixel_origin();
            crate::session::MonitorInfo {
                name: d.id.to_string(),
                x,
                y,
                w: d.px_w,
                h: d.px_h,
            }
        }
        None => crate::session::MonitorInfo {
            name: String::new(),
            x: 0,
            y: 0,
            w: 0,
            h: 0,
        },
    }
}

/// The window's rect in pixels: `(x, y, w, h)`.
///
/// # Errors
/// When the window is gone.
pub fn client_screen_rect(hwnd: isize) -> Result<(i32, i32, u32, u32), String> {
    find(hwnd)
        .map(|w| pixel_rect(&w))
        .ok_or_else(|| "the window is no longer on screen".to_string())
}

/// Enumerate selectable windows, front to back.
#[must_use]
pub fn enumerate() -> Vec<WindowInfo> {
    match vuoom_capture::shareable_windows() {
        Ok(list) => list.iter().map(to_info).collect(),
        Err(e) => {
            tracing::warn!("could not list windows: {e}");
            Vec::new()
        }
    }
}
