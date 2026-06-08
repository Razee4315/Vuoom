# 03 — Screen Capture Layer

How Vuoom captures the screen at up to 60fps/4K, keeps frames on the GPU, excludes the OS cursor
(so we can draw our own), and bridges into the wgpu compositor with zero CPU roundtrip.

---

## Decision

| Need | Choice |
|---|---|
| Primary capture | **`windows-capture` 2.0.0** (MIT) — Windows Graphics Capture wrapper |
| Full-display / >60fps / no-border fallback | **DXGI Desktop Duplication** (ships inside `windows-capture` 2.0 as `DxgiDuplicationApi`) |
| Maximum-control escape hatch | Raw `windows-rs` `Windows.Graphics.Capture` |
| GPU bridge to compositor | **NT shared-handle texture + keyed mutex** → `wgpu` DX12 |

This is the exact architecture Cap ships: `windows-capture` → `wgpu` → encoder.

---

## Why `windows-capture`

- **Most mature** Rust WGC wrapper, actively maintained (v2.0.0; was 1.5.0 mid-2025). MIT.
- **Gives you the GPU texture:** `Frame::as_raw_texture() -> &ID3D11Texture2D` and
  `as_raw_surface() -> &IDirect3DSurface`. This is *the* differentiator — frames stay on the GPU.
  CPU buffers (`buffer()`, `buffer_crop()`, `buffer_without_title_bar()`) exist when needed.
- **Cursor exclusion:** `CursorCaptureSettings` lets us exclude the OS cursor so Vuoom renders
  its own smoothed/highlighted cursor. ✅ (Core to the product feel.)
- **Border control:** `DrawBorderSettings` maps to WGC `IsBorderRequired`.
- **Dirty regions:** `DirtyRegionSettings` (Windows 11 24H2+) → skip unchanged frames (idle → 0 Hz).
- **Timestamps:** `Frame::timestamp() -> TimeSpan` (WGC `SystemRelativeTime`, QPC-based).
- **Color formats:** `ColorFormat::{Bgra8, Rgba8, Rgba16F}`. Use **`Bgra8`** for SDR (no swizzle),
  **`Rgba16F`** on HDR displays (tonemap in the compositor).
- **Selection / fps / DPI:** per-`Monitor` selection; `MinimumUpdateIntervalSettings` (default
  60fps) caps cadence; native-resolution 4K/high-DPI.

## The capture modes (spec §5.2)

- **Display** — pick a `Monitor`.
- **Window** — pick a `Window` (WGC window capture; `buffer_without_title_bar()` to trim chrome).
- **Region** — capture the display, crop to the user's rect in the compositor (cleanest; one
  capture path, region is just a crop uniform).

## GPU bridge to wgpu (the load-bearing detail)

`windows-capture` hands you a D3D11 `ID3D11Texture2D`. The compositor runs on wgpu's **DX12**
backend (a *different* device). Bridge without a CPU roundtrip:

1. Create/copy the capture texture with
   `D3D11_RESOURCE_MISC_SHARED_NTHANDLE | D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX`.
2. Export via `IDXGIResource1::CreateSharedHandle`.
3. Open it on wgpu's DX12 device and wrap with
   `wgpu::Device::create_texture_from_hal::<Dx12>(...)` (via `wgpu_hal::dx12::Device::texture_from_raw`).
4. Synchronize producer/consumer with the **keyed mutex**.
5. `CloseHandle` the NT handle yourself — wgpu won't.

This is the shared-handle/keyed-mutex pattern wgpu added for D3D interop (gfx-rs/wgpu#6161), and
the reason Cap vendors `wgpu-hal`. Recommended: **capture in D3D11, composite in wgpu DX12,
bridge per frame via a shared-handle texture pool.**

## Pixel-format pipeline

- Request **`Bgra8`** from WGC (clean BGRA8, no swizzle).
- In the compositor, sample BGRA, do all styling/zoom in RGBA/linear.
- Output stays **RGBA** end-to-end (GIF-only v1 needs no NV12/YUV conversion). The compositor's
  offscreen RGBA texture feeds both the preview WebSocket and the gifski export.
- **Lesson to keep:** do any pixel conversion **on the GPU**, never on the CPU at 4K60 — Cap's
  biggest early perf bug was CPU color conversion (15–25 ms/frame) capping them at 40–50fps. (If
  MP4 ever returns, GPU RGBA→NV12 is the path.)

## Gotchas & how we handle them

| Gotcha | Handling |
|---|---|
| **WGC "yellow border"** capture indicator | Removable only on **Windows 11** via `IsBorderRequired=false` + `RequestAccessAsync(Borderless)` + the `graphicsCaptureWithoutBorder` manifest capability. On Win10 it cannot be removed. **DXGI Desktop Duplication never draws a border** → use it for borderless full-display capture if Win10 support matters. |
| **60fps cap on some windows** | WGC syncs to a window's present interval; borderless/maximized double-buffered windows can lock to half refresh. Fine for our 60fps target; if we ever need >60 on display capture, use DXGI Desktop Duplication. |
| **HDR displays** | WGC SDR output looks washed/dim (OBS hit this). Capture `Rgba16F` and tonemap, or force-SDR. |
| **Multi-GPU laptops** | WGC works cross-GPU automatically; **DXGI Desktop Duplication must run on the display's GPU** — prefer WGC as primary for this reason. |
| **Window-capture timestamp bug** | A known WGC bug can report a wrong `SystemRelativeTime` in *window* capture mode; display capture is reliable. Prefer display+crop for region; validate timestamps for window capture. |
| **DispatcherQueue requirement** | WGC frame pool needs a `DispatcherQueue` unless created free-threaded; `windows-capture` handles this. |

## Change detection (efficiency)

- WGC `DirtyRegionMode` (windows-capture's `DirtyRegionSettings`) — **Win11 24H2+** — lets us
  skip processing unchanged frames. Great for idle CPU/battery.
- DXGI Desktop Duplication exposes richer dirty-rect + move-rect metadata in
  `DXGI_OUTDUPL_FRAME_INFO` for years — a bonus if we go that route for full-display.

## Implementation checklist (M1)

- [ ] Declare PER_MONITOR_AWARE_V2 at process start.
- [ ] Enumerate monitors/windows; build the source picker.
- [ ] Start a `windows-capture` session: `Bgra8`, cursor **excluded**, 60fps cap, border off (Win11).
- [ ] Pump frames on the capture thread; push `(texture, qpc_timestamp)` to a bounded queue.
- [ ] Stand up the shared-handle bridge to a wgpu DX12 device; render a trivial passthrough to
      disk to prove 60fps end-to-end.
- [ ] Add the DXGI Desktop Duplication fallback path behind a capability check.
- [ ] Test on a **mixed-DPI multi-monitor** setup from day one.

## Sources

- windows-capture: <https://github.com/NiiightmareXD/windows-capture> ·
  <https://docs.rs/windows-capture/latest/windows_capture/frame/struct.Frame.html> ·
  <https://docs.rs/windows-capture/latest/windows_capture/settings/enum.ColorFormat.html> ·
  60fps cap: <https://github.com/NiiightmareXD/windows-capture/discussions/48>
- Cap (uses windows-capture + wgpu): <https://github.com/CapSoftware/Cap>
- WGC API: <https://learn.microsoft.com/en-us/uwp/api/windows.graphics.capture.direct3d11captureframe?view=winrt-26100> ·
  Borderless: <https://learn.microsoft.com/en-us/uwp/api/windows.graphics.capture.graphicscapturesession.isborderrequired>
- DXGI Desktop Duplication: <https://learn.microsoft.com/en-us/windows/win32/direct3ddxgi/desktop-dup-api>
- WGC vs DXGI (OBS): <https://obsproject.com/forum/threads/windows-graphics-capture-vs-dxgi-desktop-duplication.149320/> ·
  HDR force-SDR: <https://github.com/obsproject/obs-studio/pull/7974>
- wgpu D3D shared handle: <https://github.com/gfx-rs/wgpu/pull/6161> ·
  <https://docs.rs/wgpu-hal/latest/wgpu_hal/trait.Device.html>
- QPC: <https://learn.microsoft.com/en-us/windows/win32/sysinfo/acquiring-high-resolution-time-stamps>
