<div align="center">

<img src=".github/assets/logo.png" alt="Vuoom logo" width="110" />

# Vuoom

**Screen recordings that zoom where it matters.**

A **free, open-source screen recorder for Windows** with cinematic **auto-zoom** —
record your screen, the camera glides into the action, and you export a small,
crisp **demo GIF or MP4** ready for your GitHub README, changelog, Slack, or
product post. No account, no watermark, no subscription.

[![Latest release](https://img.shields.io/github/v/release/Razee4315/Vuoom?label=download&color=e5484d)](https://github.com/Razee4315/Vuoom/releases/latest)
[![Downloads](https://img.shields.io/github/downloads/Razee4315/Vuoom/total?color=2ea44f)](https://github.com/Razee4315/Vuoom/releases)
[![CI](https://github.com/Razee4315/Vuoom/actions/workflows/ci.yml/badge.svg)](https://github.com/Razee4315/Vuoom/actions/workflows/ci.yml)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue)](./LICENSE)
![Platform: Windows 10/11](https://img.shields.io/badge/platform-Windows%2010%2F11-0078d4)

</div>

---

## Why Vuoom?

A flat recording of your whole screen makes UI text tiny and the "wow" moment
invisible. The tools that fix this — the ones that smoothly zoom into each click
like a little camera operator (Screen Studio, and friends) — are **Mac-only, paid,
or both**. Vuoom is that experience as a free Screen Studio alternative for
Windows, in a small native app:

**Record → zoom happens where you point → cut the dead air → export a GIF or MP4 you can paste anywhere.**

## Features

- 🎥 **Native capture** — Windows Graphics Capture at full resolution, full screen
  or a region (16:9 / 9:16 / 1:1 / 4:5 presets for social-ready framing). A red
  frame shows exactly what's being recorded (and never appears in it). Works on
  **any monitor** — Vuoom records the display it's sitting on — and you can
  **pause/resume** mid-take.
- 🔍 **Cinematic zoom** — press `Ctrl+Shift+Z` while recording to glide the camera
  into your cursor (and again to pull back out). Critically-damped spring motion,
  never a hard cut, never shows off-screen area. In the editor, every zoom is
  **aimable**: follow the cursor, or drag a crosshair to lock onto one spot.
- 🎞️ **A real editor, not a video NLE** — timeline with a ruler, playhead and
  drag-to-scrub; **trim** handles; **cut out** the middle bits you don't want;
  **zoom blocks** you can move, resize, re-level, add, or delete; **"Skim idle"**
  that plays dead stretches at 2–8×, plus manual speed regions; **undo/redo**
  across everything (`Ctrl+Z`).
- ✏️ **Annotations** — text labels (bold/italic, color presets), arrows, boxes and
  ellipses with fill/thickness controls. Each one gets its own timeline lane —
  drag to control when (and how long) it shows, `Ctrl+D` to duplicate.
- 👆 **Demo polish, baked in** — **click ripples** at every recorded mouse click,
  a **keystroke overlay** that shows shortcuts like `Ctrl+C` as chips (plain
  typing is never shown — passwords can't leak), and **Subtle / Studio frame
  presets** (padded backdrop, rounded corners, shadow).
- 📦 **Export GIF or MP4** — optimized GIF for READMEs (with a **live size
  estimate**), or H.264 MP4 up to 60 fps for Slack / X / YouTube — encoded by
  Windows itself, no ffmpeg. One-click **Copy** pastes the file anywhere.
- 💾 **Projects & crash recovery** — save everything as a `.vuoom` bundle; frames
  stream to disk while recording, so length isn't capped by RAM and a crash or
  accidental close offers **"Recover last session"** on the next launch.
- 🎨 **Five clean themes** — black & white first; zero purple.
- 🪶 **Lightweight** — Tauri + Rust, not Electron. The webview is just the cockpit;
  capture, compositing (wgpu), and encoding all run natively.

## Quick start

1. **[Download](https://github.com/Razee4315/Vuoom/releases/latest)** the `.msi`
   (recommended) or `.exe` installer.
   > Builds are not yet code-signed — SmartScreen will warn. Click
   > *More info → Run anyway*.
2. Press **Record**, frame your shot, hit **Start**.
3. While recording: `Ctrl+Shift+Z` to zoom in/out, **Pause** if you need a beat,
   `Ctrl+Shift+X` to stop.
4. Trim the ends, cut the fumbles, skim the idle parts, drop a text label or
   arrow, toggle click ripples.
5. **Export** → GIF or MP4 → **Copy** → paste it wherever the demo goes.

## Keyboard shortcuts

| Shortcut | Action |
|---|---|
| `Ctrl+Shift+R` | Start the record flow |
| `Ctrl+Shift+Z` | Zoom in / out at the cursor (while recording) |
| `Ctrl+Shift+X` | Stop recording (global) |
| `Space` | Play / pause |
| `←` / `→` | Scrub the playhead (`Shift` = 1s jumps, `Home`/`End` = trim bounds) |
| Arrow keys | Nudge the selected annotation (`Shift` = bigger steps) |
| `Ctrl+Z` / `Ctrl+Y` | Undo / redo any edit |
| `Ctrl+D` | Duplicate the selected annotation |
| `Delete` | Remove the selected annotation, zoom, speed region, or cut |
| `Ctrl+S` / `Ctrl+O` | Save / open a project |
| `Ctrl+E` | Export GIF / MP4 |

## How it's built

```
SolidJS + Vite (editor UI)  ←WebSocket preview←  Rust engine
                                                  ├─ vuoom-capture   Windows Graphics Capture (any monitor)
                                                  ├─ vuoom-input     global input log (QPC-stamped)
                                                  ├─ vuoom-zoom      auto-zoom planner + spring camera
                                                  ├─ vuoom-render    wgpu compositor (zoom, text, shapes, overlays)
                                                  ├─ vuoom-encode    GIF encoding + size estimation
                                                  ├─ vuoom-project   .vuoom project model (undo-able edits)
                                                  └─ app shell       MP4 (Media Foundation), disk-backed
                                                                     frame store + crash recovery
```

The same `render(t)` path drives scrubbing **and** export, so what you preview is
exactly what ships. While recording, frames stream straight to disk — clip length
is bounded by your drive, not your RAM. Design docs live in [`docs/`](./docs).

## Building from source

```sh
pnpm install
pnpm tauri dev      # run locally
pnpm tauri build    # produce installers
```

Releases are built and published by [GitHub Actions](.github/workflows/release.yml)
on every push to `main`; CI runs typecheck, clippy (deny warnings), and the full
Rust test suite.

## License

[Apache-2.0](./LICENSE). Free for everyone, forever — that's the point.
