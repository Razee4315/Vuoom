# 02 — System Architecture

How Vuoom is structured end to end: the crate layout, the master clock, the data flow from
capture to export, the Tauri app boundary, and distribution.

> Core principle (from the spec): **the webview is the cockpit, Rust is the engine.** Capture,
> compositing, and encoding never touch the webview. The webview renders UI and displays a
> streamed preview only.

---

## 1. Workspace / crate layout

A Cargo workspace keeps the engine modular and testable, with the Tauri app as a thin shell.
This mirrors Cap's proven `crates/*` decomposition (we study the layout, not the code).

```
vuoom/
├─ src/                         # Frontend (SolidJS + Vite + Tailwind)
│  ├─ routes/                   #   launcher, editor
│  ├─ workers/frame-worker.ts   #   WebGPU upload of streamed preview frames
│  └─ lib/                      #   timeline UI, controls, IPC wrappers
├─ src-tauri/                   # Tauri app shell (thin)
│  ├─ src/lib.rs                #   run(): register plugins, commands, state
│  ├─ src/commands.rs           #   #[tauri::command] surface
│  ├─ capabilities/             #   ACL permission files
│  ├─ binaries/                 #   ffmpeg/gifsicle sidecars (gitignored)
│  └─ tauri.conf.json
└─ crates/                      # The engine (pure Rust, unit-testable)
   ├─ vuoom-capture/            #   WGC/DXGI capture → GPU textures
   ├─ vuoom-input/              #   Raw Input hook → QPC-stamped event log
   ├─ vuoom-zoom/               #   auto-zoom planner + spring camera (NO GPU deps — testable)
   ├─ vuoom-render/             #   wgpu compositor (shaders, render graph)
   ├─ vuoom-encode/             #   Media Foundation / ffmpeg / gifski encoders
   ├─ vuoom-project/            #   .vuoom manifest (serde types) + timeline model
   └─ vuoom-preview/            #   offscreen readback → localhost WebSocket server
```

**Why this split:** `vuoom-zoom` and `vuoom-project` have **no GPU or OS dependencies**, so the
core auto-zoom math and the edit model are unit-testable in isolation — critical because
auto-zoom quality is the make-or-break milestone (M2).

---

## 2. The master clock — QueryPerformanceCounter (QPC)

Everything time-related uses **one clock**: QPC.

- Cache `QueryPerformanceFrequency()` once at startup (it never changes).
- Stamp every input event with `QueryPerformanceCounter()` **the instant it arrives** in the
  hook/`WM_INPUT` handler. Do **not** trust `MSLLHOOKSTRUCT.time` (it's `GetTickCount` ms,
  ~15 ms granularity, different epoch).
- WGC capture frames carry `SystemRelativeTime` (a QPC-based `TimeSpan`, 100 ns units). DXGI
  frames carry `LastPresentTime` (also QPC).
- Because input QPC and frame QPC share one axis, frame-relative event offsets are a direct
  subtraction — no calibration, no drift. This is what makes auto-zoom frame-accurate.

QPC is monotonic, sub-microsecond, and survives sleep/standby.

---

## 3. DPI & coordinate model

Get this right once, centrally, or every downstream feature inherits bugs.

- Declare **Per-Monitor-DPI-Aware-V2** (`SetProcessDpiAwarenessContext(PER_MONITOR_AWARE_V2)`).
  If not DPI-aware, Windows virtualizes coordinates and the cursor will misalign with captured
  pixels on scaled monitors.
- The Windows virtual desktop is one signed coordinate space; secondary monitors can have
  **negative** coordinates. Plan for `SM_XVIRTUALSCREEN`/`SM_CXVIRTUALSCREEN`.
- WGC produces **physical pixels**. Keep **all** input/cursor math in physical virtual-desktop
  pixels; use `GetPhysicalCursorPos` (not `GetCursorPos`) for absolute position. Only normalize
  to 0..1 against the captured monitor's physical rect when feeding the zoom planner.

---

## 4. Recording-time data flow

```
                 ┌─────────────────────────────┐
   start ───────▶│ vuoom-capture (WGC)         │── D3D11 GPU textures ─┐
                 └─────────────────────────────┘                       │
                 ┌─────────────────────────────┐                       │  (QPC-aligned)
   start ───────▶│ vuoom-input (Raw Input)     │── QPC-stamped events ─┤
                 └─────────────────────────────┘                       │
                                                                       ▼
                       Write near-lossless intermediate video (disk)  +  event log
                       (re-edits/re-exports never degrade the source)
```

- Capture and input run on **dedicated threads** (input needs its own message loop).
- During recording we persist a **near-lossless intermediate** (so the editor is non-destructive)
  plus the **raw event log** that powers auto-zoom. Both reference the same QPC timeline.

## 5. Edit/preview-time data flow

```
 timeline scrub (webview)
        │  cheap Tauri command: seekTo(frame)
        ▼
 vuoom-zoom: interpolate camera keyframes at t  ──▶ ProjectUniforms
        ▼
 vuoom-render: wgpu compositor (DX12), render ONE frame to offscreen texture
        ▼
 copy_texture_to_buffer (256-byte aligned) → map_async → RGBA bytes
        ▼
 vuoom-preview: pack [rgba][stride,h,w,frame#,t_ns] → ws://127.0.0.1:<port> (binary)
        ▼
 frame-worker.ts: WebGPU writeTexture → draw to <canvas>   (rAF loop, OffscreenCanvas)
```

**Key insight (proven by Cap): pixels never cross JSON IPC.** A scrub is just a tiny JSON
command (`seekTo`) telling Rust which frame to render; the heavy RGBA flows over a localhost
binary WebSocket. End-to-end ≈ 10–15 ms, sustained 60fps at 1080p. Full detail in
[`05-Compositing-and-Preview.md`](./05-Compositing-and-Preview.md).

## 6. Export-time data flow

```
 for each frame:
   vuoom-render: same compositor, render offscreen  (runs as fast as GPU+encoder allow)
        ▼
   GPU RGBA→NV12 conversion (shader)
        ▼
   vuoom-encode: Media Foundation HW encoder → H.264/H.265 → mux MP4
                 OR gifski collector ← RGBA (downscaled) → GIF
```

**Preview and export share one wgpu device and one compositor** — only the sink differs
(WebSocket vs encoder). This guarantees the preview is pixel-identical to the export.

---

## 7. The Tauri app boundary

| Need | Mechanism |
|---|---|
| Frontend → Rust action | `#[tauri::command]` + `invoke()` (request/response) |
| Rust → Frontend **streaming** (encode progress, recording status, preview ready) | **`tauri::ipc::Channel`** — ordered, cheap; *not* events |
| Large binary preview frames | **localhost WebSocket** (or async custom URI protocol) — never IPC |
| Long-lived recorder state | `app.manage(...)` + a task that owns the recorder, driven by channels |

Tauri 2 specifics that matter:
- **Capabilities/ACL** replace v1's allowlist — per-window permission files in `capabilities/`.
- Core APIs (fs, shell, dialog…) are **separate plugins** now — opt in to only what's used.
- JS import moved: `@tauri-apps/api/tauri` → `@tauri-apps/api/core`.
- HWND access via `window.hwnd()` or `raw-window-handle` for wgpu/overlay windows. Beware the
  known wgpu+transparency surface-contention flicker — prefer a separate child/overlay window
  for any native rendering over the webview, rather than sharing its surface.

## 8. The overlay window (region selector + countdown)

A separate transparent, frameless, always-on-top, click-through window:
`transparent: true, decorations: false, alwaysOnTop: true, skipTaskbar: true`, with
`set_ignore_cursor_events(true)` toggled off only over the draggable selection handles.

---

## 9. Distribution & signing

- **Installer:** NSIS `-setup.exe` (per-user, no admin) is the primary artifact; the updater
  reuses it. MSI optional.
- **App size:** the Tauri shell is single-digit MB (uses the OS WebView2 runtime). Any bundled
  ffmpeg sidecar dominates the download — prefer a minimal static build or first-run download.
- **SmartScreen (critical for a free download):** unsigned apps get the "unknown publisher"
  block. Recommended path for an indie/free app is **Azure Trusted Signing** (cloud HSM, cheap
  monthly) wired via `bundle.windows.signCommand`. An OV cert works but the warning persists
  until download reputation accrues; an EV cert clears it immediately but is pricier.
- **Auto-update:** `tauri-plugin-updater` + `tauri-plugin-process`; host a static `latest.json`
  manifest; build emits `*-setup.exe` + `.sig`.

---

## 10. Threading model summary

| Thread | Owns |
|---|---|
| Main / UI | Tauri event loop, webview |
| Capture thread | WGC frame pool callback → texture queue |
| Input thread | Message-only window + Raw Input pump → QPC event queue |
| Render/compositor | wgpu device, render graph (preview + export) |
| Preview WS thread | Tokio task: serve `127.0.0.1` WebSocket, "latest frame wins" |
| Encode thread(s) | Media Foundation / ffmpeg / gifski |

Inter-thread: lock-free/`mpsc` queues; never block the input hook callback (it sits on the
system-wide input path — stamp QPC, enqueue, return).

## Sources

Cross-cutting; see [`03`](./03-Capture.md)–[`06`](./06-Export.md) for primary sources. Tauri
specifics: <https://v2.tauri.app/develop/calling-frontend/>,
<https://v2.tauri.app/distribute/sign/windows/>, <https://v2.tauri.app/plugin/updater/>.
