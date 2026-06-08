<div align="center">

# Vuoom

### Screen recordings that zoom where it matters.

A **free, lightweight, native Windows** screen recorder that automatically zooms into your
cursor like a little camera operator following the action — then lets you drop in clean text
labels and arrows — and exports a polished, ready-to-post **demo GIF**. Built with
**Tauri 2 + Rust + wgpu**.

> The "Screen Studio" cinematic auto-zoom experience, brought to Windows, for free —
> purpose-built for programmers making demo GIFs for their READMEs.

</div>

---

## What Vuoom is (and isn't)

**Is:** the dead-simple way to turn a screen recording into a beautiful **demo GIF** — record,
auto-zoom does its thing, add a couple of text labels/arrows if you want, export a small,
crisp GIF for your README / PR / Reddit / X post. Looks designed by default.

**Isn't:** a full video editor. No audio, no MP4 (v1), no webcam, no cloud. GIF-first, focused,
fast. *(MP4 is a clean future add — the architecture leaves room — but it's intentionally out of v1.)*

## What this repo is right now

Vuoom is in the **direction-setting / pre-implementation** phase. This repository holds the
**product spec** and a complete, research-backed **engineering knowledge base** in
[`/docs`](./docs) so any developer (human or AI agent) can implement with a clear, validated
technical direction and zero guesswork.

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
                              + TEXT (glyphon) + arrows/highlights (lyon)
                                v
                        Final render (offscreen RGBA)
                                v
                   gifski (separate binary)  →  optimized GIF
```

The guiding principle: **the webview is the cockpit, Rust is the engine.** All heavy lifting —
capture, compositing, encoding — happens in native Rust. The Tauri webview renders only the UI.

## Documentation map

| Doc | What's in it |
|---|---|
| [`docs/Vuoom-Spec.md`](./docs/Vuoom-Spec.md) | The original product & technical spec, with the v1.1 scope amendments at the top |
| [`docs/00-Research-Index.md`](./docs/00-Research-Index.md) | Executive summary + decision log across all research |
| [`docs/01-Tech-Stack.md`](./docs/01-Tech-Stack.md) | The definitive crate/library list with versions & rationale |
| [`docs/02-Architecture.md`](./docs/02-Architecture.md) | System architecture, crate layout, the shared clock, data flow |
| [`docs/03-Capture.md`](./docs/03-Capture.md) | Screen capture: WGC, DXGI fallback, wgpu zero-copy interop |
| [`docs/04-Input-and-AutoZoom.md`](./docs/04-Input-and-AutoZoom.md) | Global input capture **+ the auto-zoom algorithm** (the core feature) |
| [`docs/05-Compositing-and-Preview.md`](./docs/05-Compositing-and-Preview.md) | wgpu render graph (incl. text/annotation passes), the preview bridge, SDF shaders |
| [`docs/06-Export.md`](./docs/06-Export.md) | **GIF export** via gifski: presets, file-size estimation, copy-to-clipboard |
| [`docs/07-Landscape-and-Positioning.md`](./docs/07-Landscape-and-Positioning.md) | Competitors, open-source reuse map, positioning |
| [`docs/08-Roadmap.md`](./docs/08-Roadmap.md) | Build milestones with acceptance criteria |
| [`docs/09-Decisions-and-Open-Questions.md`](./docs/09-Decisions-and-Open-Questions.md) | ADR-style decision records + open questions |
| [`docs/10-Licensing.md`](./docs/10-Licensing.md) | The licensing analysis + why Vuoom is Apache-2.0 and how gifski stays isolated |
| [`docs/11-Editor-and-Annotations.md`](./docs/11-Editor-and-Annotations.md) | The editing UI + simple text/arrow/highlight annotations |

## Status

- [x] Product spec (+ v1.1 scope: GIF-only, no audio, Apache-2.0, text editing)
- [x] Deep technical research (capture, input, auto-zoom, compositing, **text/editor UX**, GIF export, Tauri, landscape)
- [x] Engineering knowledge base in `/docs`
- [ ] App scaffold (`src-tauri/` + SolidJS frontend)
- [ ] M1 — Capture core
- [ ] M2 — Input log + auto-zoom planner *(the make-or-break milestone)*
- [ ] M3 — Editor + preview bridge + **text/annotations**
- [ ] M4 — Framing & **GIF export**
- [ ] M5 — Polish & first public release

## License

**Apache License 2.0** — see [`LICENSE`](./LICENSE). Vuoom's own code is permissively licensed
with an explicit patent grant. The one copyleft dependency, **gifski (AGPL)**, is deliberately
**not linked** — it ships as a separate binary invoked out-of-process, so Vuoom's source stays
Apache-2.0-clean. Full rationale in [`docs/10-Licensing.md`](./docs/10-Licensing.md).

---

<div align="center">
<sub>Vuoom — reads like "vroom" crossed with the double-o of "zoom." Developer-first, no-nonsense, free and native.</sub>
</div>
