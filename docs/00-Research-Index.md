# 00 — Research Index & Executive Summary

**Status:** Research complete · **Date:** 2026-06-08 · **Platform:** Windows 10/11 first

This document is the **single entry point** to Vuoom's engineering knowledge base. It summarizes
every major technical decision, links to the deep-dive doc for each, and records the open
questions. Everything here is backed by primary-source research (Microsoft docs, the wgpu/Tauri
repos, and direct reading of the **Cap** open-source codebase, which is the closest comparable
product). Source URLs live in each detailed doc.

---

## The one reference that matters most: Cap (cap.so)

[**Cap**](https://github.com/CapSoftware/Cap) is an open-source **Tauri 2 + Rust + wgpu**
cross-platform screen recorder that shipped Screen-Studio-style auto-zoom in **Feb 2026**. It is
almost exactly Vuoom's stack and goals. Across every research area, Cap's source code validated
the architecture we'd independently arrived at. **We study Cap heavily** — its crate layout,
its `zoom.rs` spring constants, its `frame_ws.rs` preview transport, its `enc-mediafoundation`
encoder selection.

> ⚠️ **Licensing line in the sand:** Cap's *application* code is **AGPLv3**. We may **read and
> learn** from it, but we must **never copy AGPL code** into a permissively-licensed Vuoom.
> Some Cap sub-crates (`scap`, `cap-camera*`) are MIT and *are* reusable. See
> [`10-Licensing.md`](./10-Licensing.md). This is the most important constraint in the whole project.

---

## Decision log (the TL;DR of every doc)

| Area | Decision | Primary choice | Fallback | Detail |
|---|---|---|---|---|
| **Screen capture** | Native WGC, keep frames on GPU | **`windows-capture` 2.0.0** (MIT) | DXGI Desktop Duplication (in same crate) → raw `windows-rs` WGC | [03](./03-Capture.md) |
| **GPU bridge** | Capture (D3D11) → compositor (DX12) | **NT shared-handle texture + keyed mutex** | Same-device D3D11 | [03](./03-Capture.md) |
| **Input capture** | Global, background, frame-aligned | **Raw Input via `windows-rs`** (`RIDEV_INPUTSINK`) + `GetPhysicalCursorPos` polling | `rdev` (prototype only) | [04](./04-Input-and-AutoZoom.md) |
| **Master clock** | One clock for input + frames | **QueryPerformanceCounter (QPC)** | — | [02](./02-Architecture.md) |
| **Auto-zoom motion** | Cinematic, self-correcting | **Critically-damped springs** (half-life parameterized) | Catmull-Rom for baked manual keyframes | [04](./04-Input-and-AutoZoom.md) |
| **Compositor** | GPU, one device, two sinks | **wgpu on DX12 backend**, offscreen render | Vulkan backend | [05](./05-Compositing-and-Preview.md) |
| **Live preview bridge** | 60fps without JSON IPC | **Offscreen RGBA → localhost binary WebSocket → Web Worker → WebGPU canvas** (Cap's design) | Async custom URI protocol | [05](./05-Compositing-and-Preview.md) |
| **Rounded corners + shadow** | Resolution-independent | **SDF (signed distance field) shader** | — | [05](./05-Compositing-and-Preview.md) |
| **MP4 encoding** | HW-accelerated, license-safe | **Windows Media Foundation HW encoder** (`windows-rs`) | `ffmpeg-next` w/ `h264_nvenc`/`qsv`/`amf` | [06](./06-Export.md) |
| **GIF encoding** | High quality, small | **`gifski`** (mind AGPL — process-isolate) | `gif` + `color_quant` (lower quality) | [06](./06-Export.md), [10](./10-Licensing.md) |
| **"Copy GIF"** | Actually works in chat apps | **Copy the file (CF_HDROP)** to clipboard | Custom `image/gif` clipboard format | [06](./06-Export.md) |
| **Shell / UI** | Lightweight native | **Tauri 2.x** | — | [08-app section in 02](./02-Architecture.md) |
| **Frontend framework** | Fast 60fps scrub timeline | **SolidJS + Vite + Tailwind** | Svelte 5 | [02](./02-Architecture.md) |
| **Rust→JS streaming** | Ordered, cheap progress/signals | **`tauri::ipc::Channel`** | events (small only) | [02](./02-Architecture.md) |
| **Sidecars** | ffmpeg/gifsicle if used | **Tauri `externalBin` + shell plugin** | — | [06](./06-Export.md) |
| **Distribution** | Avoid SmartScreen on a free app | **NSIS installer + Azure Trusted Signing** (cloud HSM) | OV cert (reputation builds over time) | [02](./02-Architecture.md) |
| **Vuoom's own license** | Free + permissive | **MIT or Apache-2.0** (decision pending) | — | [10](./10-Licensing.md) |

---

## The five hardest problems (and where they're solved)

1. **Auto-zoom that looks professional, not janky** → the make-or-break feature. Solved with
   click-clustering + critically-damped springs + a camera clamp that never reveals off-screen
   area. Cap's exact spring constants are documented. → [`04`](./04-Input-and-AutoZoom.md)

2. **Getting 60fps preview frames to the webview** without choking JSON IPC → offscreen wgpu
   render → RGBA readback → **localhost binary WebSocket** → Web Worker → WebGPU canvas. Proven by
   Cap at sustained 60fps, ~10–15ms end-to-end. → [`05`](./05-Compositing-and-Preview.md)

3. **Zero-copy capture→GPU→encode** → keep frames as D3D11 textures; bridge to the wgpu DX12
   compositor via an NT shared handle + keyed mutex; convert RGBA→NV12 **on the GPU** before the
   hardware encoder. → [`03`](./03-Capture.md), [`05`](./05-Compositing-and-Preview.md)

4. **License-safe, patent-safe, fast MP4 export** → use the **OS/GPU hardware encoder** (Media
   Foundation) so codec patent royalties are the OS/GPU vendor's responsibility and no x264/x265
   (GPL) is shipped. → [`06`](./06-Export.md), [`10`](./10-Licensing.md)

5. **Frame-accurate input↔video sync across mixed-DPI multi-monitor** → one QPC clock for both;
   Per-Monitor-DPI-Aware-V2; store all input in physical virtual-desktop pixels. → [`02`](./02-Architecture.md), [`04`](./04-Input-and-AutoZoom.md)

---

## Open questions carried forward

These are tracked with full context in [`09-Decisions-and-Open-Questions.md`](./09-Decisions-and-Open-Questions.md):

1. **Vuoom's license** — MIT vs Apache-2.0 (affects how we isolate gifski/AGPL).
2. **GIF licensing path** — open-source the GIF module, buy gifski's commercial license, or
   invoke gifski as an out-of-process binary? (Legal sign-off needed.)
3. **Audio in v1 or deferred?** Architecture supports it now; spec defers it. Confirm.
4. **Minimum Windows version** — Win10 1809+ vs Win11-only. Borderless capture (no yellow
   border) and dirty-region APIs need Win11; decide whether those are launch features.
5. **Preview at 4K** — raw RGBA over WebSocket is ~500 MB/s at 1080p60; for 4K preview, drop to
   half-res or switch that path to H.264 + WebCodecs. Decide threshold.

---

## How to use this knowledge base (for the implementing agent)

- Read this index, then [`01-Tech-Stack.md`](./01-Tech-Stack.md) and [`02-Architecture.md`](./02-Architecture.md) for the lay of the land.
- Before writing the make-or-break milestone, internalize [`04-Input-and-AutoZoom.md`](./04-Input-and-AutoZoom.md) — it contains the actual algorithm and default parameters.
- Every doc ends with a **Sources** section. Claims are cited; verify anything load-bearing
  against the live source before hard-coding (especially Cap's constants — they iterate fast).
- The build order and acceptance gates are in [`08-Roadmap.md`](./08-Roadmap.md).
