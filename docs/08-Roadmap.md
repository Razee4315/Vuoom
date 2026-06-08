# 08 — Build Roadmap & Milestones

The build order, with explicit acceptance gates. **M2 (auto-zoom) is the make-or-break
milestone** — its quality decides whether Vuoom is a real product or just another recorder. Ship
M1–M4 as the first public release.

---

## M0 — Scaffold & spikes (de-risk before building)

Goal: prove the two scariest unknowns work on real hardware before committing to the full app.

- [ ] `npm create tauri-app` (SolidJS + Vite + TS) → workspace with `crates/` per
      [`02-Architecture.md`](./02-Architecture.md).
- [ ] Tailwind v4 wired into Vite. Tauri 2 capabilities baseline.
- [ ] **Spike 1 — capture→GPU:** `windows-capture` session → `as_raw_texture()` → shared-handle
      bridge → wgpu DX12 → render passthrough to disk. *Prove 60fps at 1080p.*
- [ ] **Spike 2 — preview bridge:** offscreen wgpu render → readback → localhost WebSocket →
      Web Worker → WebGPU `<canvas>`. *Prove a moving test pattern at 60fps in the webview.*
- [ ] Decide the open questions in [`09`](./09-Decisions-and-Open-Questions.md) that block M1
      (min Windows version, license).

**Gate:** both spikes hit 60fps on a mid-range machine. If the preview bridge can't, fall back to
the async custom-URI-protocol variant before proceeding.

## M1 — Capture core

Goal: rock-solid capture of display / window / region.

- [ ] PER_MONITOR_AWARE_V2; monitor/window enumeration + source picker.
- [ ] `windows-capture`: `Bgra8`, cursor **excluded**, 60fps cap, border off (Win11).
- [ ] Region = display capture + crop uniform.
- [ ] Write a **near-lossless intermediate** to disk; basic playback.
- [ ] DXGI Desktop Duplication fallback behind a capability check (Win10 / borderless full-display).
- [ ] **Test on mixed-DPI multi-monitor from day one.**

**Gate:** sustained 60fps at 1080p; graceful 4K (30fps acceptable initially); correct on
mixed-DPI multi-monitor; recording start ≤ ~2 s.

## M2 — Input log + auto-zoom planner ⭐ (make-or-break)

Goal: the signature cinematic auto-zoom, looking "like Screen Studio."

- [ ] `vuoom-input`: Raw Input thread (message-only window, `RIDEV_INPUTSINK`), QPC-stamped event
      log, `GetPhysicalCursorPos` poll. Align to WGC `SystemRelativeTime`.
- [ ] `vuoom-zoom` (no GPU/OS deps): planner (click-cluster + debounce + hold + frequency limit)
      → `ZoomKeyframe`s; critically-damped spring camera; off-screen clamp; jitter dead-zone +
      cursor pre-smoothing. Defaults per [`04`](./04-Input-and-AutoZoom.md).
- [ ] Compositor applies the camera transform on **export** (editor preview comes in M3).
- [ ] **Unit tests** against synthetic event logs: no off-screen reveal, no >N zooms/sec, debounce.
- [ ] Tune defaults against ≥10 real recordings.

**Gate (spec §5.1):** (1) 60fps, no stutter; (2) non-expert calls default output "professional /
like Screen Studio"; (3) never shows off-screen empty area; (4) zooms editable; (5) micro-moves /
accidental clicks cause no jumps.

## M3 — Editor + preview bridge + text/annotations

Goal: a responsive, clean editor with frame-accurate scrubbing and simple annotations.

- [ ] Productionize the M0 preview bridge (pooled readback buffers, latest-frame-wins, OffscreenCanvas).
- [ ] Editor shell: single-window canvas + timeline + context-sensitive properties panel
      (see [`11`](./11-Editor-and-Annotations.md)).
- [ ] Timeline: scrub (`seekTo` command), frame-accurate preview, trim/cut, per-segment speed.
- [ ] Edit zoom regions (add/remove/move/resize/re-target) on the same `ZoomKeyframe` list.
- [ ] **Text labels** (glyphon): click-to-add, inline edit, drag/resize, font/size/color, timeline
      time-range + fade. **Arrow + highlight box** (lyon). Keep it simple.
- [ ] `.vuoom` project save/reopen (serde manifest + intermediate reference); all edits are
      parameters re-rendered by one `render(t)` path.

**Gate:** smooth scrubbing without dropped frames on a mid-range machine; a text label can be added
and edited in under ~10 s by a first-time user; edits round-trip through save/reopen.

## M4 — Framing & GIF export (→ first public release)

Goal: make it look designed, and get a small, crisp GIF out.

- [ ] Compositor: background (solid/gradient/image/blur), padding, rounded corners (SDF), shadow.
- [ ] Aspect-ratio presets (16:9 / 9:16 / 1:1) with live reframe.
- [ ] **GIF export:** gifski **out-of-process**; README/HQ presets; **live size estimate**
      (sample-and-extrapolate); **copy-GIF-as-file** to clipboard; reveal in Explorer. Optional
      gifsicle "shrink more" pass.
- [ ] *(Out of v1: MP4, audio. WebP is an optional later add.)*

**Gate:** default **README GIF** is small (low-MB) and good; a 30 s clip exports well under
real-time on a typical GPU; the one-click flow (record → auto-zoom → export) produces a postable
GIF **with zero manual editing**.

## M5 — Polish

- [ ] Sensible zero-config defaults tuned for first-run delight.
- [ ] Global hotkeys (start/stop/pause), system tray, countdown + region-selector overlay window.
- [ ] Sensitivity controls, speed regions, click ripple / cursor highlight toggles.
- [ ] Code signing (Azure Trusted Signing), NSIS installer, auto-updater, `latest.json` hosting.

## Later (post-v1, architecture already supports)

**MP4 export** (Media Foundation HW encoder — research preserved in git history); audio tracks;
webcam PiP; macOS/Linux builds; cloud/sharing; AI features. Hold the
[spec §7 + v1.1 amendments](./Vuoom-Spec.md) out-of-scope line for v1.

---

## Risk register (carry through every milestone)

| Risk | Milestone | Mitigation |
|---|---|---|
| Auto-zoom feels janky / motion-sick | M2 | Heavy easing/debounce/frequency tuning vs real recordings; the §5.1 gate |
| Preview bridge perf | M0/M3 | Decided up front (WebSocket); spike before the editor; URI-protocol fallback |
| Mixed-DPI / multi-monitor coord bugs | M1 | Test mixed-DPI from M1; everything in physical px |
| GIF too large | M4 | gifski + presets + sample-extrapolate size estimate |
| gifski AGPL | M4 | ✅ Resolved — ship gifski as an **out-of-process binary** (Vuoom stays Apache-2.0); bundle AGPL text. [`10`](./10-Licensing.md) |
| Text/annotation rendering quality | M3 | glyphon (proven) + lyon; rasterize text at export resolution |
| gifski/gifsicle distribution | M4/M5 | Ship as sidecars; bundle license texts |
| SmartScreen on free download | M5 | Azure Trusted Signing |
| Scope creep | all | Hold the spec §7 line |
