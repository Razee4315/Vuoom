//! A drag "wall" that keeps the recording panel out of the recorded region.
//!
//! Ground truth (verified on real hardware): on Windows 10 a capture-excluded window
//! (`WDA_EXCLUDEFROMCAPTURE`) that overlaps the captured area is recorded as a solid BLACK
//! rectangle — Win10 does not re-composite the desktop behind the excluded window the way
//! Win11 does. So exclusion alone is NOT enough on Win10: the panel must physically stay
//! outside the recorded region. (The region-border strips are safe only because they sit
//! just OUTSIDE the crop, not because exclusion reveals what's behind them.)
//!
//! Rather than let the user drop the panel inside the region and then snap it away (jarring,
//! and the reason the earlier debounced-snap approach was reverted), we constrain the drag
//! itself. We subclass the main window and clamp `WM_MOVING`: Windows hands us the proposed
//! window rect during its own modal move loop (the frontend's drag region initiates it via
//! `WM_NCLBUTTONDOWN`/HTCAPTION), we push that rect out of the forbidden zone along the axis
//! of least penetration and write it back, returning TRUE. The panel slides along the region
//! edge like it hit a wall — flicker-free and native-feeling.
//!
//! The subclass proc is a raw C callback, so it reads a process-global snapshot of the
//! forbidden rect (set at install time). The region is fixed for the recording's duration,
//! so a snapshot is correct. Install/remove are marshaled onto the UI thread with
//! `run_on_main_thread` because `SetWindowSubclass` must run on the window's owning thread
//! and the record-flow commands are `async` (they run off the main thread).

#[cfg(windows)]
mod imp {
    use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
    use tauri::{AppHandle, Manager};
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
    use windows::Win32::UI::WindowsAndMessaging::WM_MOVING;

    /// Our stable subclass id on the main window (arbitrary; must match across install/remove).
    const SUBCLASS_ID: usize = 0x7600_0001;

    /// Forbidden rect (virtual-desktop physical px) + whether the wall is armed. Read by the
    /// subclass proc, written by install/remove.
    static ARMED: AtomicBool = AtomicBool::new(false);
    static F_LEFT: AtomicI32 = AtomicI32::new(0);
    static F_TOP: AtomicI32 = AtomicI32::new(0);
    static F_RIGHT: AtomicI32 = AtomicI32::new(0);
    static F_BOTTOM: AtomicI32 = AtomicI32::new(0);

    /// Clamp a proposed window rect so it never intersects the forbidden rect, pushing it out
    /// along the axis of least penetration and keeping the other axis where the user is
    /// dragging (natural wall-sliding). No-op when there's no intersection.
    fn clamp(rect: &mut RECT) {
        let (fl, ft, fr, fb) = (
            F_LEFT.load(Ordering::Relaxed),
            F_TOP.load(Ordering::Relaxed),
            F_RIGHT.load(Ordering::Relaxed),
            F_BOTTOM.load(Ordering::Relaxed),
        );
        if fr <= fl || fb <= ft {
            return; // no/empty forbidden rect
        }
        let (l, t, r, b) = (rect.left, rect.top, rect.right, rect.bottom);
        // No overlap → free move.
        if r <= fl || l >= fr || b <= ft || t >= fb {
            return;
        }
        let (w, h) = (r - l, b - t);
        // How far to travel to escape past each side of the forbidden rect.
        let push_left = r - fl; // move left until the panel's right edge sits at fl
        let push_right = fr - l; // move right until the panel's left edge sits at fr
        let push_up = b - ft; // move up until the panel's bottom edge sits at ft
        let push_down = fb - t; // move down until the panel's top edge sits at fb
        let pen_x = push_left.min(push_right);
        let pen_y = push_up.min(push_down);
        if pen_x <= pen_y {
            // Resolve horizontally; keep the vertical position the user is dragging to.
            let new_l = if push_left <= push_right { fl - w } else { fr };
            rect.left = new_l;
            rect.right = new_l + w;
        } else {
            let new_t = if push_up <= push_down { ft - h } else { fb };
            rect.top = new_t;
            rect.bottom = new_t + h;
        }
    }

    unsafe extern "system" fn subclass_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _id: usize,
        _ref: usize,
    ) -> LRESULT {
        if msg == WM_MOVING && ARMED.load(Ordering::Relaxed) {
            // lParam points to the proposed window RECT (screen coords, physical px because
            // the process is per-monitor-DPI-aware). Clamp it in place and report TRUE.
            // SAFETY: for WM_MOVING, lParam is a valid `*mut RECT` owned by the move loop.
            let rect = unsafe { &mut *(lparam.0 as *mut RECT) };
            clamp(rect);
            return LRESULT(1); // TRUE — we adjusted the rect
        }
        unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
    }

    /// Arm the wall around `forbidden` (virtual-desktop physical px `(x, y, w, h)`) and
    /// subclass the main window. Safe to call repeatedly (updates the forbidden rect).
    pub fn install(app: &AppHandle, forbidden: (i32, i32, i32, i32)) {
        let (x, y, w, h) = forbidden;
        F_LEFT.store(x, Ordering::Relaxed);
        F_TOP.store(y, Ordering::Relaxed);
        F_RIGHT.store(x + w, Ordering::Relaxed);
        F_BOTTOM.store(y + h, Ordering::Relaxed);
        ARMED.store(true, Ordering::Relaxed);
        let app = app.clone();
        // SetWindowSubclass must run on the window's owning (UI) thread.
        let _ = app.clone().run_on_main_thread(move || {
            if let Some(w) = app.get_webview_window("main") {
                if let Ok(h) = w.hwnd() {
                    // SAFETY: standard subclass install on a realized top-level HWND.
                    unsafe {
                        let _ = SetWindowSubclass(HWND(h.0), Some(subclass_proc), SUBCLASS_ID, 0);
                    }
                }
            }
        });
    }

    /// Disarm the wall and drop the subclass. Disarms synchronously (the proc becomes a
    /// pass-through immediately); the actual `RemoveWindowSubclass` runs on the UI thread.
    pub fn remove(app: &AppHandle) {
        ARMED.store(false, Ordering::Relaxed);
        let app = app.clone();
        let _ = app.clone().run_on_main_thread(move || {
            if let Some(w) = app.get_webview_window("main") {
                if let Ok(h) = w.hwnd() {
                    // SAFETY: removing our own subclass by matching proc + id.
                    unsafe {
                        let _ = RemoveWindowSubclass(HWND(h.0), Some(subclass_proc), SUBCLASS_ID);
                    }
                }
            }
        });
    }
}

#[cfg(windows)]
pub use imp::{install, remove};

/// Non-Windows stubs (keep `cargo check` portable; the app is Windows-only).
#[cfg(not(windows))]
pub fn install(_app: &tauri::AppHandle, _forbidden: (i32, i32, i32, i32)) {}

#[cfg(not(windows))]
pub fn remove(_app: &tauri::AppHandle) {}
