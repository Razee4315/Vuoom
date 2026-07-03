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
| **Real timeline** (ruler, auto-zoom segment track, annotation track, playhead, drag-scrub) | ✅ |
| **Trim** (draggable in/out handles; playback + export honor it) | 🟡 |
| **Speed-up dead time** (auto-detects idle >2.5s from the event log, plays at 3×, remapped in export) | 🟡 |
| **Zoom editing** (drag blocks to retime, drag edges to resize, strength slider, add-at-playhead, delete; camera re-simulates) | 🟡 |
| **Export panel**: live size estimate (sample-and-extrapolate), progress bar, copy-GIF-file (CF_HDROP), copy path, reveal in Explorer | 🟡 |
| **Global stop hotkey** (Ctrl+Shift+X while recording, backend-watched) + Ctrl+Shift+R to record | 🟡 |
| **System tray** (Open / Quit) | 🟡 |
| **Non-blocking engine boot** (splash shows while GPU/preview server start) | 🟡 |
| Export is borderless by default (framing removed; old bundles normalized on open) | ✅ |

### Deliberately not built (overrides the older docs)

- **Framing/background/padding controls** — feature removed by owner decision (borderless exports).
- **Editor aspect-ratio reframe** — covered by the record-time 16:9 / 9:16 / 1:1 / 4:5 region
  presets; a post-hoc reframe without framing would distort or crop.
- **MP4 / audio / WebP / webcam / click-ripple / pause** — out of v1 per the spec amendments
  (cursor is excluded from capture by design, so a ripple has nothing to anchor to).
- **Code signing / auto-updater** — infrastructure, pre-public-launch task.
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
| Compositor **text** annotations (glyphon) | `vuoom-render` | 🟡 |
| End-to-end wiring (record → capture+input → project → preview → export GIF) | `src-tauri` | 🟡 |
| Frontend preview canvas + Record/Stop/Export + timeline scrub | `src/` | 🟡 |
| Frontend annotation editing (text/arrow/box on canvas → re-render) | `src/` | 🟡 |

**Every planned feature is now implemented.** What remains is purely *runtime verification on a
real Windows machine* (the 🟡 layers) — capture, GPU rendering, and input can't be exercised on
a GPU-less CI runner, so they're compile-verified here and confirmed by running the app.

---

## AI Demo Director (MCP) — agent-driven demo generation

An AI agent (Claude) can drive Vuoom to generate a demo: drive a target app, record with
auto-zoom, *see* the output via sampled frames, and re-record to improve it. Setup in
`docs/AI_DEMO_DIRECTOR.md`; research in `docs/13-AI-Demo-Director-Research.md`.

| Area | Crate | Status |
|---|---|---|
| Control protocol (newline-JSON req/resp, client, port discovery) | `vuoom-control` | ✅ |
| Input injection (SendInput move/click/type/key/scroll) + coord/key math | `vuoom-input` | ✅ runtime-verified |
| In-app control server (opt-in `VUOOM_ENABLE_CONTROL`, loopback) | `src-tauri` | ✅ runtime-verified |
| `Session::sample_frames` (composite → base64 PNG) + `clip_info` | `src-tauri` | ✅ runtime-verified |
| MCP sidecar — 20 tools over stdio (rmcp) | `vuoom-mcp` | ✅ runtime-verified |
| Click-to-zoom for agent recordings (`SetAutoZoomOnClick`, default on) | all | ✅ runtime-verified |

**Runtime-verified on a real Windows machine (2026-06-14):** a sequential MCP agent drove the
live app end to end — `start_recording` → injected clicks/typing into a target app (Notepad) →
`stop_recording` (158 frames, **1 cinematic zoom planned from the injected clicks**) →
`get_frames` (real composited PNGs) → `export_gif` (real GIF written). The one runtime bug found
and fixed: click-to-zoom defaults off for the interactive UI, so the agent path now opts it on.

**v2 (2026-07-02, from the `mcp_improve.md` review — 🟡 needs a fresh runtime pass):**

| Area | Crate | Status |
|---|---|---|
| Humanized injection: min-jerk cursor glide, settle-then-click, paced typing, stepped scroll, drag | `vuoom-input` | 🟡 compile+unit-tested |
| Injection failure detection (UIPI short-count → error), extended keys, OEM punctuation | `vuoom-input` | 🟡 |
| Live `screenshot` op + `status` / `cancel_recording` | all | 🟡 |
| `clip_state` returns zoom/cut/speed **spans** + `output_duration` | all | 🟡 |
| Output-timeline `get_frames`/`seek` (matches export through trim/cuts/speed) | `src-tauri` | 🟡 |
| Edit ops over the protocol (zoom/cut/speed/trim repair without re-recording) | all | 🟡 |
| Job-based exports with `export_status` polling | all | 🟡 |
| Auth token + connect/read timeouts + discovery-file cleanup + `VUOOM_ENABLE_CONTROL=0` off | all | ✅ CI (auth integration test) |
| Per-recording `set_zoom_style` + agent flags reset at stop/cancel | `src-tauri` | 🟡 |

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

Logo: the Vuoom mark (bold rounded **V** with a recording-dot tip). Canonical vector in
`src/assets/logo.svg` / `src/Logo.tsx` (inline, theme-aware); app icons generated from
`src-tauri/icons/source-1024.png` via `pnpm tauri icon`.
