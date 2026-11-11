//! macOS window shaping for the record flow, and the region border.
//!
//! The record flow reuses the one main window (see `commands.rs`): it covers the display for
//! the region selector, shrinks to a floating panel while recording, and returns to the
//! editor. On Windows that is fullscreen + always-on-top. On a Mac, fullscreen slides the
//! window into a Space of its own (an animation, and the frozen backdrop would sit in the
//! wrong Space), so the window is instead made borderless, laid over the whole display at a
//! level above the menu bar and the Dock, and kept on every Space. The editor gets its own
//! title bar (native traffic lights over the app's title bar) back afterwards.
//!
//! None of these windows needs hiding from capture: ScreenCaptureKit's filter leaves every
//! window of this app out of the recording.

use objc2::rc::Retained;
use objc2::MainThreadMarker;
use objc2_app_kit::{
    NSBackingStoreType, NSColor, NSWindow, NSWindowButton, NSWindowCollectionBehavior,
    NSWindowStyleMask,
};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

/// `NSScreenSaverWindowLevel`: above the menu bar, the Dock and other apps' panels.
const COVER_LEVEL: isize = 1000;
/// `NSFloatingWindowLevel`: the recording panel floats over other apps.
const PANEL_LEVEL: isize = 3;
/// `NSNormalWindowLevel`.
const NORMAL_LEVEL: isize = 0;

/// The editor window's own style, kept from the first time it is reshaped.
static EDITOR_STYLE: AtomicUsize = AtomicUsize::new(usize::MAX);

/// A window handle that may cross to the main thread.
#[derive(Clone, Copy)]
struct Handle(usize);

fn handle(window: &tauri::WebviewWindow) -> Option<Handle> {
    window.ns_window().ok().map(|p| Handle(p as usize))
}

/// Run `f` on the main thread with the NSWindow behind `window`.
fn with_ns(window: &tauri::WebviewWindow, f: impl FnOnce(&NSWindow, MainThreadMarker) + Send) {
    let Some(h) = handle(window) else { return };
    dispatch2::run_on_main(move |mtm| {
        // SAFETY: the pointer is Tauri's live NSWindow for this webview window, used on the
        // main thread only.
        let ns = unsafe { &*(h.0 as *const NSWindow) };
        f(ns, mtm);
    });
}

fn remember_style(ns: &NSWindow) {
    let current = ns.styleMask().0;
    let _ =
        EDITOR_STYLE.compare_exchange(usize::MAX, current, Ordering::Relaxed, Ordering::Relaxed);
}

fn traffic_lights(ns: &NSWindow, visible: bool) {
    for b in [
        NSWindowButton::CloseButton,
        NSWindowButton::MiniaturizeButton,
        NSWindowButton::ZoomButton,
    ] {
        if let Some(button) = ns.standardWindowButton(b) {
            button.setHidden(!visible);
        }
    }
}

/// A rect in the global desktop space (points, origin top-left of the main display) as a
/// Cocoa frame (origin bottom-left of the main display).
fn cocoa_rect(x: f64, y: f64, w: f64, h: f64) -> CGRect {
    let main_h = vuoom_input::mac::displays()
        .iter()
        .find(|d| d.primary)
        .map_or(h, |d| d.h);
    CGRect::new(CGPoint::new(x, main_h - (y + h)), CGSize::new(w, h))
}

/// The display (points) whose pixel origin and size match a Tauri monitor's.
fn monitor_points(m: &tauri::Monitor) -> Option<(f64, f64, f64, f64)> {
    let key = crate::displays::monitor_key(m)?;
    let d = vuoom_input::mac::displays()
        .into_iter()
        .find(|d| d.id.to_string() == key)?;
    Some((d.x, d.y, d.w, d.h))
}

/// Lay the window over the whole of `monitor` (menu bar and Dock included), on every Space,
/// borderless and unmovable: the region selector's canvas.
pub fn cover_display(window: &tauri::WebviewWindow, monitor: Option<&tauri::Monitor>) {
    let area = monitor.and_then(monitor_points).or_else(|| {
        vuoom_input::mac::displays()
            .into_iter()
            .find(|d| d.primary)
            .map(|d| (d.x, d.y, d.w, d.h))
    });
    let Some((x, y, w, h)) = area else { return };
    let frame = cocoa_rect(x, y, w, h);
    with_ns(window, move |ns, _| {
        remember_style(ns);
        ns.setStyleMask(NSWindowStyleMask::Borderless);
        ns.setLevel(COVER_LEVEL);
        ns.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::FullScreenAuxiliary
                | NSWindowCollectionBehavior::Stationary,
        );
        ns.setMovable(false);
        ns.setFrame_display(frame, true);
    });
}

/// The recording panel: borderless, floating over other apps (fullscreen ones too), on
/// every Space, dragged by its own drag region.
pub fn panel(window: &tauri::WebviewWindow) {
    with_ns(window, |ns, _| {
        remember_style(ns);
        ns.setStyleMask(NSWindowStyleMask::Borderless);
        ns.setLevel(PANEL_LEVEL);
        ns.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::FullScreenAuxiliary,
        );
        ns.setMovable(true);
        ns.setHasShadow(true);
    });
}

/// Back to the editor: its own title bar and traffic lights, a normal level, one Space.
pub fn editor(window: &tauri::WebviewWindow) {
    with_ns(window, |ns, _| {
        let style = EDITOR_STYLE.load(Ordering::Relaxed);
        if style != usize::MAX {
            ns.setStyleMask(NSWindowStyleMask(style));
        }
        ns.setLevel(NORMAL_LEVEL);
        ns.setCollectionBehavior(NSWindowCollectionBehavior::Default);
        ns.setMovable(true);
        traffic_lights(ns, true);
    });
}

// ── region border ─────────────────────────────────────────────────────────────────────

/// Border thickness in points.
const THICKNESS: f64 = 2.0;

thread_local! {
    /// The strip windows of each shown border, by id. Main thread only.
    static BORDERS: RefCell<HashMap<u64, Vec<Retained<NSWindow>>>> = RefCell::new(HashMap::new());
}

static NEXT_BORDER: AtomicU64 = AtomicU64::new(1);

/// A visible frame around the area being recorded: four thin click-through strips just
/// outside it. Dropping it removes them.
pub struct RegionBorder {
    id: u64,
}

impl RegionBorder {
    /// Frame the rect `x, y, w, h` (pixels, the space input events use).
    pub fn show(x: i32, y: i32, w: i32, h: i32) -> Option<Self> {
        let list = vuoom_input::mac::displays();
        let (cx, cy) = (f64::from(x + w / 2), f64::from(y + h / 2));
        // The display holding the rect's center gives the pixel → point scale.
        let d = list
            .iter()
            .find(|d| {
                let (ox, oy) = d.pixel_origin();
                let (ox, oy) = (f64::from(ox), f64::from(oy));
                cx >= ox && cx < ox + f64::from(d.px_w) && cy >= oy && cy < oy + f64::from(d.px_h)
            })
            .or_else(|| list.iter().find(|d| d.primary))?;
        let s = d.scale;
        let (px, py, pw, ph) = (
            f64::from(x) / s,
            f64::from(y) / s,
            f64::from(w) / s,
            f64::from(h) / s,
        );
        let t = THICKNESS;
        let strips = [
            (px - t, py - t, pw + 2.0 * t, t),  // top
            (px - t, py + ph, pw + 2.0 * t, t), // bottom
            (px - t, py, t, ph),                // left
            (px + pw, py, t, ph),               // right
        ];
        let frames: Vec<CGRect> = strips
            .iter()
            .map(|&(sx, sy, sw, sh)| cocoa_rect(sx, sy, sw, sh))
            .collect();
        let id = NEXT_BORDER.fetch_add(1, Ordering::Relaxed);
        dispatch2::run_on_main(move |mtm| {
            let windows: Vec<Retained<NSWindow>> = frames
                .iter()
                .map(|&frame| {
                    // SAFETY: creating a plain borderless window on the main thread.
                    let win = unsafe {
                        NSWindow::initWithContentRect_styleMask_backing_defer(
                            mtm.alloc(),
                            frame,
                            NSWindowStyleMask::Borderless,
                            NSBackingStoreType::Buffered,
                            false,
                        )
                    };
                    // SAFETY: we own the window through `Retained`, not through `close`.
                    unsafe { win.setReleasedWhenClosed(false) };
                    // `--record` red (#e5484d), present but not shouting.
                    let red = NSColor::colorWithSRGBRed_green_blue_alpha(
                        229.0 / 255.0,
                        72.0 / 255.0,
                        77.0 / 255.0,
                        0.75,
                    );
                    win.setBackgroundColor(Some(&red));
                    win.setOpaque(false);
                    win.setHasShadow(false);
                    win.setIgnoresMouseEvents(true);
                    win.setLevel(COVER_LEVEL);
                    win.setCollectionBehavior(
                        NSWindowCollectionBehavior::CanJoinAllSpaces
                            | NSWindowCollectionBehavior::FullScreenAuxiliary
                            | NSWindowCollectionBehavior::Stationary
                            | NSWindowCollectionBehavior::IgnoresCycle,
                    );
                    win.orderFrontRegardless();
                    win
                })
                .collect();
            BORDERS.with(|b| b.borrow_mut().insert(id, windows));
        });
        Some(Self { id })
    }
}

impl Drop for RegionBorder {
    fn drop(&mut self) {
        let id = self.id;
        dispatch2::run_on_main(move |_| {
            if let Some(windows) = BORDERS.with(|b| b.borrow_mut().remove(&id)) {
                for w in windows {
                    w.orderOut(None);
                    w.close();
                }
            }
        });
    }
}
