# 01 — Tech Stack & Library Selection

The definitive, research-validated list of crates and tools for Vuoom, with versions, licenses,
and the one-line reason each was chosen. Deep rationale lives in the per-area docs.

> Versions are "as of mid-2026." Pin exact versions in `Cargo.toml`/`package.json` at scaffold
> time and re-verify against crates.io/npm. Where a crate moves fast, prefer the latest minor.

---

## Rust core (`src-tauri` + workspace crates)

| Concern | Crate | Version | License | Why |
|---|---|---|---|---|
| App shell / IPC | `tauri` | 2.x | MIT/Apache-2.0 | Native, lightweight, webview UI + Rust engine |
| Screen capture | **`windows-capture`** | 2.0.x | MIT | Most mature Rust WGC wrapper; exposes `ID3D11Texture2D` (GPU), cursor exclusion, border control, dirty regions, QPC timestamps. *Cap uses this on Windows.* |
| OS bindings | `windows` (windows-rs) | 0.60+ | MIT/Apache-2.0 | Raw Input, Media Foundation, DXGI, DPI, clipboard, QPC |
| GPU compositing | `wgpu` | 25.x | MIT/Apache-2.0 | Cross-platform GPU; DX12 backend on Windows; shaders for zoom/pan/frame styling |
| GPU HAL interop | `wgpu-hal` | (matches wgpu) | MIT/Apache-2.0 | `create_texture_from_hal` for shared-handle D3D interop |
| Image helpers | `image`, `fast_image_resize` | latest | MIT/Apache-2.0 | Downscale frames (Lanczos) for GIF/thumbnails |
| Color/pixels | `imgref`, `rgb` | latest | MIT/Apache-2.0 | gifski frame interface (`Img<RGBA8>`) |
| GIF encoder | **`gifski`** | 1.34.x | **AGPL-3.0** ⚠️ | Best-in-class GIF palettes; **process-isolate or relicense** — see [10](./10-Licensing.md) |
| MP4 (fallback) | `ffmpeg-next` *(or `rsmpeg`)* | latest | LGPL build only | HW encoders (`h264_nvenc`/`qsv`/`amf`); use **LGPL** ffmpeg, never x264/x265 |
| Async runtime | `tokio` | latest | MIT | Tauri async tasks, channels, WebSocket server |
| WebSocket (preview) | `tokio-tungstenite` | latest | MIT | Localhost binary frame transport to the webview |
| Serialization | `serde`, `serde_json` | latest | MIT/Apache-2.0 | `.vuoom` project manifest, IPC payloads |
| Logging | `tracing`, `tracing-subscriber` | latest | MIT | Structured diagnostics |
| Errors | `thiserror`, `anyhow` | latest | MIT/Apache-2.0 | Library vs app error ergonomics |
| Window handle | `raw-window-handle` | 0.6 | MIT/Apache-2.0 | HWND for wgpu surface / overlay window |
| Math | `glam` | latest | MIT/Apache-2.0 | Vectors/matrices for camera transforms & SDF math |

### Encoding: the chosen path

- **Primary MP4 path = native Media Foundation** via `windows-rs`
  (`Win32::Media::MediaFoundation`, `MFTEnumEx` hardware-first). No external libs, copyleft-safe,
  patent-safe (Windows holds the codec license). Mirrors Cap's `enc-mediafoundation`.
- **Fallback MP4 path = `ffmpeg-next` bindings** with hardware encoders ordered by detected GPU
  vendor (`h264_amf` → `h264_mf` → `h264_nvenc` → `h264_qsv`), software `libx264` **last** and
  only if licensing is accepted. Use a **BtbN `lgpl-shared`** ffmpeg build if linking.
- **GIF = `gifski`** (collector/writer thread model), optional `gifsicle` second pass.

## Tauri plugins

| Plugin | Purpose |
|---|---|
| `tauri-plugin-shell` | Spawn ffmpeg/gifsicle **sidecars** with piped stdin/stdout |
| `tauri-plugin-global-shortcut` | Start/stop/pause hotkeys |
| `tauri-plugin-positioner` | Tray-relative window placement |
| `tauri-plugin-single-instance` | **Register first** — prevent two capture engines |
| `tauri-plugin-autostart` | Launch on login (optional) |
| `tauri-plugin-updater` + `tauri-plugin-process` | Auto-update + relaunch |
| `tauri-plugin-dialog` / `tauri-plugin-fs` | Save dialogs, project files |
| `tauri-plugin-opener` | Reveal-in-Explorer |

System tray is built into Tauri 2 core (`tray-icon` feature) — no separate plugin.

## Frontend (`src/`)

| Concern | Choice | Why |
|---|---|---|
| Framework | **SolidJS** | Fine-grained reactivity, no VDOM diff — ideal for a 60fps scrubbing timeline. (Svelte 5 is the close second.) |
| Build | **Vite** | Tauri's default dev server integration |
| Styling | **Tailwind CSS v4** (`@tailwindcss/vite`) | Fast, consistent editor UI |
| Preview render | **WebGPU** (primary) + Canvas2D `putImageData` (fallback) | Upload streamed RGBA frames to a `<canvas>` texture in a Web Worker |
| Timeline render | **`<canvas>`** for ruler/waveform/thumbnails | Thousands of DOM nodes would kill scrub perf |

## Sidecar binaries (bundled, not crates)

| Binary | Use | License note |
|---|---|---|
| `ffmpeg.exe` (optional) | Fallback encode / WebP / container muxing | Ship **LGPL** build (BtbN `lgpl-shared`); **never** GPL/x264 |
| `gifsicle.exe` (optional) | Second-pass GIF size optimization | GPL-compatible as a separate invoked process |

Naming: Tauri requires a target-triple suffix, e.g.
`src-tauri/binaries/ffmpeg-x86_64-pc-windows-msvc.exe`. Keep these out of git (see `.gitignore`);
fetch via a setup script.

## Tooling

- **Rust** stable (MSRV pin once scaffolded), `cargo`, `clippy`, `rustfmt`.
- **Node** LTS, `pnpm` (or `npm`).
- **Tauri CLI** (`@tauri-apps/cli` 2.x).
- **Windows SDK** (for Media Foundation / D3D / DXGI headers via windows-rs — usually no manual
  install needed; windows-rs ships the bindings).

---

## What we deliberately did NOT choose

| Rejected | Reason |
|---|---|
| `scap` (as the capture layer) | CPU-only BGRA frames, no GPU texture handle → forces a GPU→CPU→GPU roundtrip. Even Cap (scap's authors) don't use it for the Windows recording path. |
| `CrabGrab` | Only "minimal maintenance"; cursor/border/timestamp control not surfaced. (Good to mine its wgpu-interop code as a reference, though.) |
| `device_query` for input | Polling only, no event stream / no clicks-as-events. |
| Electron / browser capture pipeline | The exact quality ceiling Vuoom exists to beat. |
| Bundling **x264/x265** | GPL would force open-sourcing all of Vuoom, plus patent exposure. HW encoders avoid both. |
| Putting preview frames over JSON IPC | Serialization chokes at 60fps; 1080p BGRA ≈ 8 MB/frame. |

## Sources

See the per-area docs (`03`–`06`, `02`) for the full sourced rationale behind each row.
