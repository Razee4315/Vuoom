# Implementation Status

Living snapshot of what's built. Updated as work lands. See `08-Roadmap.md` for the plan.

Legend: ✅ implemented + **CI-verified** (compiles, clippy-clean, unit tests pass) ·
🟡 implemented + **compile-verified only** (needs a real run on a Windows machine to confirm
runtime behaviour — CI has no GPU/display and the project rule is CI-only builds) · ⬜ not started.

---

## Core logic (pure, fully CI-verified)

| Area | Crate | Status |
|---|---|---|
| Auto-zoom planner (click-cluster, debounce, hold, frequency-limit) | `vuoom-zoom` | ✅ |
| Spring camera + off-screen clamp + deterministic per-frame track | `vuoom-zoom` | ✅ |
| Timeline edit ops (insert/move/resize/remove keyframes) | `vuoom-zoom` | ✅ |
| `.vuoom` project model + JSON round-trip | `vuoom-project` | ✅ |
| Annotation timing / fade opacity | `vuoom-project` | ✅ |
| Framing (background, padding, corners, shadow, aspect dims) | `vuoom-project` | ✅ |
| Speed-region time remapping (source ↔ played) | `vuoom-project` | ✅ |
| GIF frame-planning + size estimation | `vuoom-encode` | ✅ |
| GIF export orchestration (PNG write → gifski → gifsicle, out-of-process) | `vuoom-encode` | ✅ |
| Compositor layout math (camera src/dst rects, corners) | `vuoom-render` | ✅ |
| Scene builder (project + camera → GPU draw list) | `vuoom-render` | ✅ |
| Preview wire-protocol (RGBA + LE trailer) | `vuoom-preview` | ✅ |
| Input: QPC clock, DPI awareness, raw→normalized bridge | `vuoom-input` | ✅ |
| Tauri command surface (config, project save/load, presets, size estimate) | `src-tauri` | ✅ |

## App shell & UI (CI-verified: types + build)

| Area | Status |
|---|---|
| Editor shell (titlebar · toolbar · tool rail · canvas · properties · timeline) | ✅ |
| **Black & white + neutral themes** (Mono Dark/Light, Graphite, Paper, Midnight; no purple) | ✅ |
| **Custom frameless titlebar** + min/max/close window controls | ✅ |
| Tauri UI hardening (drag region, context-menu, anti-flash startup, window permissions) | ✅ |
| CI/CD pipeline (lint+test+build) + Release pipeline (installers) | ✅ |
| Published installer | ✅ **`v0.1.1`** — redesigned black/white UI + custom titlebar (install this one) |

## Integration (compile-verified; runtime needs your machine)

| Area | Crate | Status |
|---|---|---|
| Global input recorder (low-level hooks + pump thread) | `vuoom-input` | 🟡 |
| Localhost WebSocket preview server ("latest wins") | `vuoom-preview` | 🟡 |
| WGC screen capture (windows-capture) | `vuoom-capture` | 🟡 |
| wgpu compositor — headless device + offscreen render + readback | `vuoom-render` | 🟡 |
| wgpu compositor — composite pipeline (bg + zoom/pan crop + rounded-corner SDF) | `vuoom-render` | 🟡 |
| Compositor shape annotations (highlight boxes + arrows) | `vuoom-render` | 🟡 |
| Compositor **text** annotations (glyphon) | `vuoom-render` | ⬜ (model + scene-resolution ready; GPU text pass remaining) |
| End-to-end wiring (record → capture+input → project → preview → export GIF) | `src-tauri` | 🟡 |
| Frontend preview canvas + Record/Stop/Export + timeline scrub | `src/` | 🟡 |
| Frontend annotation editing (place text/arrow/box on canvas → re-render) | `src/` | 🟡 (boxes/arrows render; text stored, awaiting glyphon) |

---

## Why the 🟡 / ⬜ boundary

The capture, GPU compositor, and live preview need a **real GPU + display** to verify (does it
capture at 60fps? does the zoom render correctly?). CI runners have neither, and the project rule
is *no local builds*. So those layers are written to **compile cleanly on CI** but their runtime
correctness is confirmed by **running the app on the Windows machine** — the same loop as
installing the release. As these land, expect a short "install this build and tell me what you
see" step.

## Testing the app (the runtime checks)

Once a build with the wired pipeline is installed, the things to verify on a real machine:

1. **Record** → click "Record", interact with your screen (click around), then "Stop". The
   status bar should report the duration + number of auto-zooms detected.
2. **Preview/scrub** → drag the timeline slider. The canvas should show the composited frame
   (background + zoomed-in source) for that moment. *This exercises capture → compositor →
   WebSocket preview end-to-end.*
3. **Export GIF** → needs the `gifski` binary on PATH for now (sidecar bundling is a TODO);
   writes `vuoom-demo.gif`.

Report back: does capture produce frames? does the zoom render correctly? does the preview
stream smoothly? Those answers drive the next fixes (this layer is compile-verified, not yet
run-verified).

## How to get an installable build

The **Release** workflow (manual) builds `.msi` + `.exe` and publishes to
[Releases](https://github.com/Razee4315/Vuoom/releases). It auto-bumps the patch version. Trigger:
Actions → *Release* → *Run workflow* (or `gh workflow run release.yml`). Installs unsigned for now
→ SmartScreen → *More info → Run anyway* (code signing comes before public launch).

## Assets

Logo / icons are placeholders (default Tauri icons). Drop-in replacement when provided.
