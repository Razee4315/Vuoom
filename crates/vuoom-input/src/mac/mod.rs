//! macOS input plumbing: the host clock, display geometry (points → pixels), the global
//! cursor position, the key-code table, key state polling and the permission checks the
//! event tap needs. See `docs/19-macOS-Port-Plan.md` section 4.
//!
//! Coordinates: Windows code keeps every position in physical pixels of one virtual
//! desktop. macOS reports positions in points (origin top-left of the main display, y down)
//! while ScreenCaptureKit delivers pixels. A point maps to pixels through the display it is
//! on: `px = pt * scale`. A display's own origin maps the same way, so a position relative
//! to the recorded display comes out as `(pt - origin) * scale`, exactly the pixel offset
//! inside its frames. Every display of a take uses its own scale, so mixed 2x/1x setups
//! line up too.

mod cursor;
mod keymap;
mod recorder;

pub(crate) use cursor::changed as cursor_changed;
pub use keymap::{mac_to_vk, vk_to_mac};
pub use recorder::InputRecorder;

use objc2_core_foundation::{CGPoint, CGRect};
use objc2_core_graphics::{
    CGDirectDisplayID, CGDisplayBounds, CGDisplayCopyDisplayMode, CGDisplayMode, CGEvent,
    CGEventFlags, CGEventSource, CGEventSourceStateID, CGGetActiveDisplayList, CGMainDisplayID,
};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Nanoseconds on the host clock (`mach_absolute_time`), the clock every capture sample
/// on macOS is stamped with.
#[must_use]
#[allow(deprecated)] // libc points at the mach2 crate; the functions themselves are fine.
pub fn host_time_ns() -> i64 {
    use std::sync::OnceLock;
    static BASE: OnceLock<(u64, u64)> = OnceLock::new();
    let &(numer, denom) = BASE.get_or_init(|| {
        let mut info = libc::mach_timebase_info { numer: 0, denom: 0 };
        // SAFETY: `info` is a valid, writable struct for the call.
        unsafe { libc::mach_timebase_info(&mut info) };
        if info.denom == 0 {
            (1, 1)
        } else {
            (u64::from(info.numer), u64::from(info.denom))
        }
    });
    // SAFETY: a plain clock read.
    let ticks = unsafe { libc::mach_absolute_time() };
    (u128::from(ticks) * u128::from(numer) / u128::from(denom)) as i64
}

/// One attached display, in points (the global desktop space) and in pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Display {
    pub id: CGDirectDisplayID,
    /// Origin and size in points.
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    /// Pixel size (what a capture of it delivers).
    pub px_w: u32,
    pub px_h: u32,
    /// Pixels per point (2 on a Retina panel).
    pub scale: f64,
    pub primary: bool,
}

impl Display {
    /// The display's origin in the pixel space positions are reported in.
    #[must_use]
    pub fn pixel_origin(&self) -> (i32, i32) {
        (
            (self.x * self.scale).round() as i32,
            (self.y * self.scale).round() as i32,
        )
    }

    fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
}

/// Every active display, the main one first.
#[must_use]
pub fn displays() -> Vec<Display> {
    let mut ids = [0 as CGDirectDisplayID; 16];
    let mut count = 0u32;
    // SAFETY: `ids` holds 16 entries and we pass that capacity.
    let err = unsafe { CGGetActiveDisplayList(16, ids.as_mut_ptr(), &mut count) };
    if err.0 != 0 {
        return Vec::new();
    }
    let main = CGMainDisplayID();
    let mut out: Vec<Display> = ids[..count as usize]
        .iter()
        .map(|&id| display_info(id, main))
        .collect();
    out.sort_by_key(|d| !d.primary);
    out
}

fn display_info(id: CGDirectDisplayID, main: CGDirectDisplayID) -> Display {
    let b: CGRect = CGDisplayBounds(id);
    let mode = CGDisplayCopyDisplayMode(id);
    let mut px_w = CGDisplayMode::pixel_width(mode.as_deref()) as u32;
    let mut px_h = CGDisplayMode::pixel_height(mode.as_deref()) as u32;
    if px_w == 0 || px_h == 0 {
        px_w = b.size.width.round() as u32;
        px_h = b.size.height.round() as u32;
    }
    let scale = if b.size.width > 0.0 {
        f64::from(px_w) / b.size.width
    } else {
        1.0
    };
    Display {
        id,
        x: b.origin.x,
        y: b.origin.y,
        w: b.size.width,
        h: b.size.height,
        px_w,
        px_h,
        scale,
        primary: id == main,
    }
}

/// The display list, refreshed at most once a second: the event tap converts every mouse
/// event, and asking CoreGraphics each time would be wasted work.
fn cached_displays() -> Vec<Display> {
    static CACHE: Mutex<Option<(Instant, Vec<Display>)>> = Mutex::new(None);
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let fresh = cache
        .as_ref()
        .is_some_and(|(at, _)| at.elapsed() < Duration::from_secs(1));
    if !fresh {
        *cache = Some((Instant::now(), displays()));
    }
    cache.as_ref().map(|(_, d)| d.clone()).unwrap_or_default()
}

/// The pixels-per-point of the display showing `(x, y)` (points), else the main one's.
fn scale_at(list: &[Display], x: f64, y: f64) -> f64 {
    list.iter()
        .find(|d| d.contains(x, y))
        .or_else(|| list.iter().find(|d| d.primary))
        .map_or(1.0, |d| d.scale)
}

/// A global position in points → the pixel space positions are reported in.
#[must_use]
pub fn to_pixels(x: f64, y: f64) -> (i32, i32) {
    let scale = scale_at(&cached_displays(), x, y);
    pixels_with(scale, x, y)
}

fn pixels_with(scale: f64, x: f64, y: f64) -> (i32, i32) {
    ((x * scale).round() as i32, (y * scale).round() as i32)
}

/// The pointer's position now, in pixels (see the module docs).
#[must_use]
pub fn cursor_pos() -> Option<(i32, i32)> {
    let event = CGEvent::new(None)?;
    let p: CGPoint = CGEvent::location(Some(&event));
    Some(to_pixels(p.x, p.y))
}

/// Which modifiers are held right now.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub cmd: bool,
}

impl Modifiers {
    pub(crate) fn from_flags(f: CGEventFlags) -> Self {
        Self {
            ctrl: f.contains(CGEventFlags::MaskControl),
            shift: f.contains(CGEventFlags::MaskShift),
            alt: f.contains(CGEventFlags::MaskAlternate),
            cmd: f.contains(CGEventFlags::MaskCommand),
        }
    }
}

/// The modifiers held right now, by the system's key state.
#[must_use]
pub fn modifiers_down() -> Modifiers {
    Modifiers::from_flags(CGEventSource::flags_state(
        CGEventSourceStateID::CombinedSessionState,
    ))
}

/// Whether the key with this Windows virtual-key code is held right now.
#[must_use]
pub fn vk_down(vk: u16) -> bool {
    vk_to_mac(vk).is_some_and(|code| {
        CGEventSource::key_state(CGEventSourceStateID::CombinedSessionState, code)
    })
}

/// Whether Vuoom may install an event tap that can swallow keys (Accessibility). With
/// `prompt`, macOS shows its "allow in System Settings" dialog when it may not.
#[must_use]
pub fn accessibility_trusted(prompt: bool) -> bool {
    use objc2_core_foundation::{CFBoolean, CFDictionary, CFString};
    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrustedWithOptions(options: *const CFDictionary) -> bool;
        static kAXTrustedCheckOptionPrompt: &'static CFString;
    }
    // SAFETY: the key is a framework constant; the dictionary lives for the call.
    unsafe {
        let value = if prompt {
            CFBoolean::new(true)
        } else {
            CFBoolean::new(false)
        };
        let dict = CFDictionary::from_slices(&[kAXTrustedCheckOptionPrompt], &[value]);
        AXIsProcessTrustedWithOptions((&*dict as *const CFDictionary<CFString, CFBoolean>).cast())
    }
}

/// Whether Vuoom may listen to global input (Input Monitoring). With `prompt`, macOS asks.
#[must_use]
pub fn input_monitoring_granted(prompt: bool) -> bool {
    #[link(name = "IOKit", kind = "framework")]
    extern "C" {
        fn IOHIDCheckAccess(request: u32) -> u32;
        fn IOHIDRequestAccess(request: u32) -> bool;
    }
    const LISTEN_EVENT: u32 = 1; // kIOHIDRequestTypeListenEvent
    const GRANTED: u32 = 0; // kIOHIDAccessTypeGranted
                            // SAFETY: plain permission queries.
    unsafe {
        if IOHIDCheckAccess(LISTEN_EVENT) == GRANTED {
            return true;
        }
        prompt && IOHIDRequestAccess(LISTEN_EVENT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(x: f64, y: f64, w: f64, h: f64, scale: f64, primary: bool) -> Display {
        Display {
            id: 0,
            x,
            y,
            w,
            h,
            px_w: (w * scale) as u32,
            px_h: (h * scale) as u32,
            scale,
            primary,
        }
    }

    #[test]
    fn points_map_to_pixels_through_their_own_display() {
        // A 2x laptop panel at the origin and a 1x monitor to its left.
        let list = [
            d(0.0, 0.0, 1512.0, 982.0, 2.0, true),
            d(-1920.0, 0.0, 1920.0, 1080.0, 1.0, false),
        ];
        let laptop = scale_at(&list, 100.0, 50.0);
        assert_eq!(pixels_with(laptop, 100.0, 50.0), (200, 100));
        let external = scale_at(&list, -1000.0, 10.0);
        assert_eq!(pixels_with(external, -1000.0, 10.0), (-1000, 10));
        // Relative to the recorded display, a position is its pixel offset in the frames.
        let (ox, oy) = list[1].pixel_origin();
        assert_eq!((-1000 - ox, 10 - oy), (920, 10));
        // Off every display falls back to the main one's scale.
        assert!((scale_at(&list, 5000.0, 5000.0) - 2.0).abs() < 1e-9);
    }

    #[test]
    fn the_host_clock_moves_forward() {
        let a = host_time_ns();
        std::thread::sleep(Duration::from_millis(2));
        assert!(host_time_ns() - a >= 1_000_000);
    }
}
