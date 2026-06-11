//! A visible frame around the area being recorded.
//!
//! Four thin native strip windows (no webviews — spawning extra WebView2 windows proved
//! unreliable, see `commands.rs`) drawn just OUTSIDE the capture region so the user always
//! sees what is being recorded. They are click-through, never take focus, and are excluded
//! from capture via `WDA_EXCLUDEFROMCAPTURE`, so the frame itself can never land in the
//! recording — belt and suspenders on top of sitting outside the crop rect.

#[cfg(windows)]
mod imp {
    use std::sync::mpsc;
    use std::thread::JoinHandle;
    use windows::core::{w, PCWSTR};
    use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::Graphics::Gdi::CreateSolidBrush;
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
        PostThreadMessageW, RegisterClassW, SetLayeredWindowAttributes, SetWindowDisplayAffinity,
        ShowWindow, TranslateMessage, LWA_ALPHA, MSG, SW_SHOWNOACTIVATE, WDA_EXCLUDEFROMCAPTURE,
        WM_QUIT, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
        WS_EX_TRANSPARENT, WS_POPUP,
    };

    /// Strip thickness in physical pixels.
    const THICKNESS: i32 = 3;
    /// `--record` red (#e5484d) as a COLORREF (0x00BBGGRR).
    const COLOR: u32 = 0x004D48E5;
    /// Strip opacity (0–255) — present but not shouting.
    const ALPHA: u8 = 190;

    unsafe extern "system" fn wndproc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    }

    /// RAII handle: the strips live while this exists, vanish when it drops.
    pub struct RegionBorder {
        tid: u32,
        thread: Option<JoinHandle<()>>,
    }

    impl RegionBorder {
        /// Show a frame around the region `(x, y, w, h)` (physical px, virtual-screen
        /// coords). Returns `None` if any window could not be created.
        pub fn show(x: i32, y: i32, w: i32, h: i32) -> Option<Self> {
            let (tx, rx) = mpsc::channel::<Option<u32>>();
            let thread = std::thread::spawn(move || unsafe {
                let class = w!("VuoomRegionBorder");
                let hinstance = match GetModuleHandleW(PCWSTR::null()) {
                    Ok(h) => h,
                    Err(_) => {
                        let _ = tx.send(None);
                        return;
                    }
                };
                let wc = WNDCLASSW {
                    lpfnWndProc: Some(wndproc),
                    hInstance: hinstance.into(),
                    lpszClassName: class,
                    hbrBackground: CreateSolidBrush(COLORREF(COLOR)),
                    ..Default::default()
                };
                // 0 with ERROR_CLASS_ALREADY_EXISTS is fine on the second recording.
                let _ = RegisterClassW(&wc);

                let t = THICKNESS;
                // Entirely OUTSIDE the region, so even a same-screen capture can't see them.
                let rects = [
                    (x - t, y - t, w + 2 * t, t), // top
                    (x - t, y + h, w + 2 * t, t), // bottom
                    (x - t, y, t, h),             // left
                    (x + w, y, t, h),             // right
                ];
                let mut hwnds: Vec<HWND> = Vec::with_capacity(4);
                for (rx_, ry, rw, rh) in rects {
                    let hwnd = CreateWindowExW(
                        WS_EX_LAYERED
                            | WS_EX_TRANSPARENT
                            | WS_EX_TOPMOST
                            | WS_EX_TOOLWINDOW
                            | WS_EX_NOACTIVATE,
                        class,
                        PCWSTR::null(),
                        WS_POPUP,
                        rx_,
                        ry,
                        rw,
                        rh,
                        None,
                        None,
                        Some(hinstance.into()),
                        None,
                    );
                    let Ok(hwnd) = hwnd else {
                        for hw in &hwnds {
                            let _ = DestroyWindow(*hw);
                        }
                        let _ = tx.send(None);
                        return;
                    };
                    let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), ALPHA, LWA_ALPHA);
                    // The frame must never appear in the recording.
                    let _ = SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE);
                    let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                    hwnds.push(hwnd);
                }

                // Windows exist → this thread has a message queue; safe to post to it now.
                let _ = tx.send(Some(GetCurrentThreadId()));
                let mut msg = MSG::default();
                while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                    let _ = TranslateMessage(&msg);
                    let _ = DispatchMessageW(&msg);
                }
                for hw in &hwnds {
                    let _ = DestroyWindow(*hw);
                }
            });

            match rx.recv() {
                Ok(Some(tid)) => Some(Self {
                    tid,
                    thread: Some(thread),
                }),
                _ => {
                    let _ = thread.join();
                    None
                }
            }
        }
    }

    impl Drop for RegionBorder {
        fn drop(&mut self) {
            // SAFETY: posting WM_QUIT to a thread with a live message queue.
            unsafe {
                let _ = PostThreadMessageW(self.tid, WM_QUIT, WPARAM(0), LPARAM(0));
            }
            if let Some(t) = self.thread.take() {
                let _ = t.join();
            }
        }
    }
}

#[cfg(windows)]
pub use imp::RegionBorder;

/// Non-Windows stub (keeps `cargo check` portable; the app is Windows-only).
#[cfg(not(windows))]
pub struct RegionBorder;

#[cfg(not(windows))]
impl RegionBorder {
    pub fn show(_x: i32, _y: i32, _w: i32, _h: i32) -> Option<Self> {
        None
    }
}
