//! Display enumeration for the record-flow source picker (macOS).
//!
//! The same surface as the Windows module: a display's `name` is the key the capture path
//! matches on, here the `CGDirectDisplayID` as text. Positions and sizes are in the pixel
//! space input events use (`vuoom_input::mac`): the origin is the display's point origin
//! times its scale, the size its pixel size, which is also how Tauri reports a monitor.

use serde::Serialize;

/// One selectable capture display.
#[derive(Debug, Clone, Serialize)]
pub struct DisplayInfo {
    /// `CGDirectDisplayID` as text, the key the capture path matches on.
    pub name: String,
    /// 1-based index for a friendly label ("Display 1"), the main display first.
    pub index: usize,
    /// Origin (pixels, see the module docs).
    pub x: i32,
    pub y: i32,
    /// Pixel size.
    pub w: u32,
    pub h: u32,
    pub primary: bool,
}

/// Every active display, the main one first.
#[must_use]
pub fn enumerate() -> Vec<DisplayInfo> {
    vuoom_input::mac::displays()
        .into_iter()
        .enumerate()
        .map(|(i, d)| {
            let (x, y) = d.pixel_origin();
            DisplayInfo {
                name: d.id.to_string(),
                index: i + 1,
                x,
                y,
                w: d.px_w,
                h: d.px_h,
                primary: d.primary,
            }
        })
        .collect()
}

/// Find an enumerated display by its key.
#[must_use]
pub fn find_by_name(name: &str) -> Option<DisplayInfo> {
    enumerate().into_iter().find(|d| d.name == name)
}

/// The capture key of a Tauri monitor. Tauri names macOS monitors by their localized
/// product name, so it is matched to a display by geometry instead.
#[must_use]
pub fn monitor_key(m: &tauri::Monitor) -> Option<String> {
    let (x, y) = (m.position().x, m.position().y);
    let (w, h) = (m.size().width, m.size().height);
    let all = enumerate();
    all.iter()
        .find(|d| d.x == x && d.y == y && d.w == w && d.h == h)
        // Rounding can differ by a pixel on fractional scales: the nearest origin wins.
        .or_else(|| {
            all.iter()
                .min_by_key(|d| (d.x - x).unsigned_abs() + (d.y - y).unsigned_abs())
        })
        .map(|d| d.name.clone())
}
