<div align="center">

<img src=".github/assets/logo.png" alt="Vuoom logo" width="110" />

# Vuoom

**Screen recordings that zoom where it matters.**

A **free, open-source screen recorder for Windows** with cinematic **auto-zoom** —
record your screen, the camera glides into the action, and you export a small,
crisp **demo GIF** ready for your GitHub README, changelog, or product post.
No account, no watermark, no subscription.

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

**Record → zoom happens where you point → trim the dead air → export a GIF you can paste anywhere.**

## Features

- 🎥 **Native capture** — Windows Graphics Capture at full resolution, full screen
  or a region (16:9 / 9:16 / 1:1 / 4:5 presets for social-ready framing).
- 🔍 **Cinematic zoom** — press `Ctrl+Shift+Z` while recording to glide the camera
  into your cursor (and again to pull back out). Critically-damped spring motion,
  never a hard cut, never shows off-screen area.
- 🎞️ **A real editor, not a video NLE** — timeline with a ruler, playhead and
  drag-to-scrub; **trim** handles; **zoom blocks** you can move, resize, re-level,
  add, or delete; one-click **"Skim idle"** that detects dead stretches and plays
  them at 3×.
- ✏️ **Annotations** — text labels, arrows, and highlight boxes. Draw them on the
  canvas, drag their bars on the timeline to control when (and how long) they show.
- 📦 **GIF export that respects your README** — README and High-quality presets, a
  **live size estimate** as you tune fps/width/quality, a real progress bar, and
  one-click **Copy GIF** (pastes as a file into Slack, Discord, GitHub comments).
- 💾 **Projects** — save a recording with all its edits as a `.vuoom` bundle and
  reopen it later. Everything is non-destructive.
- 🎨 **Five clean themes** — black & white first; zero purple.
- 🪶 **Lightweight** — Tauri + Rust, not Electron. The webview is just the cockpit;
  capture, compositing (wgpu), and encoding all run natively.

## Quick start

1. **[Download](https://github.com/Razee4315/Vuoom/releases/latest)** the `.msi`
   (recommended) or `.exe` installer.
   > Builds are not yet code-signed — SmartScreen will warn. Click
   > *More info → Run anyway*.
2. Press **Record**, frame your shot, hit **Start**.
3. While recording: `Ctrl+Shift+Z` to zoom in/out, `Ctrl+Shift+X` to stop.
4. Trim, skim the idle parts, drop a text label or arrow.
5. **Export GIF** → **Copy GIF** → paste it wherever the demo goes.

## Keyboard shortcuts

| Shortcut | Action |
|---|---|
| `Ctrl+Shift+R` | Start the record flow |
| `Ctrl+Shift+Z` | Zoom in / out at the cursor (while recording) |
| `Ctrl+Shift+X` | Stop recording (global) |
| `Space` | Play / pause |
| `Delete` | Remove the selected annotation or zoom |
| `Ctrl+S` / `Ctrl+O` | Save / open a project |
| `Ctrl+E` | Export GIF |

## How it's built

```
SolidJS + Vite (editor UI)  ←WebSocket preview←  Rust engine
                                                  ├─ vuoom-capture   Windows Graphics Capture
                                                  ├─ vuoom-input     global input log (QPC-stamped)
                                                  ├─ vuoom-zoom      auto-zoom planner + spring camera
                                                  ├─ vuoom-render    wgpu compositor (zoom, text, shapes)
                                                  ├─ vuoom-encode    GIF encoding + size estimation
                                                  └─ vuoom-project   .vuoom project model
```

The same `render(t)` path drives scrubbing **and** export, so what you preview is
exactly what ships. Design docs live in [`docs/`](./docs).

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
