<div align="center">

# Vuoom

### Screen recordings that zoom where it matters.

A **free, lightweight, native Windows** screen recorder that automatically zooms into your
cursor like a little camera operator following the action — and exports clean, polished
**MP4 / GIF** product demos. Built with **Tauri 2 + Rust + wgpu**.

> The "Screen Studio" cinematic auto-zoom experience, brought to Windows, for free.

</div>

---

## What this repo is right now

Vuoom is in the **direction-setting / pre-implementation** phase. This repository currently
holds the **product spec** and a complete, research-backed **engineering knowledge base** in
[`/docs`](./docs) so that any developer (human or AI agent) can pick up implementation with a
clear, validated technical direction and zero guesswork.

**Start here:** [`docs/00-Research-Index.md`](./docs/00-Research-Index.md) — the executive
summary and decision log that ties all the research together.

## The 30-second technical picture

```
[Windows Graphics Capture]  +  [Global Raw Input: clicks/keys/cursor + QPC timestamps]
        (windows-capture)                    (windows-rs Raw Input)
            |                                       |
       GPU texture (D3D11)                   Timestamped event log
            |                                       |
            +-------------------+-------------------+
                                v
                  Auto-zoom planner  →  camera keyframes (spring physics)
                                v
            wgpu compositor (DX12): zoom/pan + background + rounded corners + shadow
                    |                               |
                    v                               v
          Live preview                        Final render
      (offscreen → WebSocket →            (offscreen → RGBA→NV12 →
       Web Worker → WebGPU canvas)         Media Foundation / ffmpeg → MP4,  gifski → GIF)
```

The guiding principle: **the webview is the cockpit, Rust is the engine.** All heavy lifting —
capture, compositing, encoding — happens in native Rust. The Tauri webview renders only UI.

## Documentation map

| Doc | What's in it |
|---|---|
| [`docs/Vuoom-Spec.md`](./docs/Vuoom-Spec.md) | The original product & technical spec (source of truth for *what* we build) |
| [`docs/00-Research-Index.md`](./docs/00-Research-Index.md) | Executive summary + decision log across all research |
| [`docs/01-Tech-Stack.md`](./docs/01-Tech-Stack.md) | The definitive crate/library list with versions & rationale |
| [`docs/02-Architecture.md`](./docs/02-Architecture.md) | System architecture, crate layout, the shared clock, data flow |
| [`docs/03-Capture.md`](./docs/03-Capture.md) | Screen capture: WGC, DXGI fallback, wgpu zero-copy interop |
| [`docs/04-Input-and-AutoZoom.md`](./docs/04-Input-and-AutoZoom.md) | Global input capture **+ the auto-zoom algorithm** (the core feature) |
| [`docs/05-Compositing-and-Preview.md`](./docs/05-Compositing-and-Preview.md) | wgpu render graph, the preview-bridge solution, SDF shaders |
| [`docs/06-Export.md`](./docs/06-Export.md) | MP4 (Media Foundation/ffmpeg) + GIF (gifski), presets, file-size estimation |
| [`docs/07-Landscape-and-Positioning.md`](./docs/07-Landscape-and-Positioning.md) | Competitors, open-source reuse map, positioning |
| [`docs/08-Roadmap.md`](./docs/08-Roadmap.md) | Build milestones with acceptance criteria |
| [`docs/09-Decisions-and-Open-Questions.md`](./docs/09-Decisions-and-Open-Questions.md) | ADR-style decision records + open questions |
| [`docs/10-Licensing.md`](./docs/10-Licensing.md) | The licensing analysis (AGPL/codec/gifski) + Vuoom's own license |

## Status

- [x] Product spec
- [x] Deep technical research (capture, input, auto-zoom, compositing, encoding, Tauri, landscape)
- [x] Engineering knowledge base in `/docs`
- [ ] App scaffold (`src-tauri/` + frontend)
- [ ] M1 — Capture core
- [ ] M2 — Input log + auto-zoom planner *(the make-or-break milestone)*
- [ ] M3 — Editor + preview bridge
- [ ] M4 — Framing & export
- [ ] M5 — Polish & first public release

## License

To be finalized — see [`docs/10-Licensing.md`](./docs/10-Licensing.md). The leading candidate is
a permissive license (MIT/Apache-2.0) for Vuoom's own code, with careful isolation of any
copyleft dependencies (notably gifski/AGPL) behind a process boundary.

---

<div align="center">
<sub>Vuoom — reads like "vroom" crossed with the double-o of "zoom." Developer-first, no-nonsense, free and native.</sub>
</div>
