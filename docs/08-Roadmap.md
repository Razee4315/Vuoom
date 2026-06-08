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

## M3 — Editor + preview bridge

Goal: a responsive editor with frame-accurate scrubbing.

- [ ] Productionize the M0 preview bridge (pooled readback buffers, latest-frame-wins, OffscreenCanvas).
- [ ] Timeline: scrub (`seekTo` command), frame-accurate preview, trim/cut, per-segment speed.
- [ ] Edit zoom regions (add/remove/move/resize/re-target) on the same `ZoomKeyframe` list.
- [ ] `.vuoom` project save/reopen (serde manifest + intermediate reference).

**Gate:** smooth scrubbing without dropped frames on a mid-range machine; edits round-trip through
save/reopen.

## M4 — Framing & export (→ first public release)

Goal: make it look designed, and get it out as MP4/GIF.

- [ ] Compositor: background (solid/gradient/image/blur), padding, rounded corners (SDF), shadow.
- [ ] Aspect-ratio presets (16:9 / 9:16 / 1:1) with live reframe.
- [ ] **Export:** Media Foundation H.264 (primary), ffmpeg HW fallback; **gifski** GIF with
      README/HQ presets; estimated file size; **copy-GIF-as-file** to clipboard; reveal in Explorer.
- [ ] WebP opt-in export.

**Gate:** a 30 s clip exports to MP4 well under real-time on a typical GPU; default README GIF is
small (low-MB) and good; one-click flow (record → auto-zoom → export) produces a postable result
**with zero manual editing**.

## M5 — Polish

- [ ] Sensible zero-config defaults tuned for first-run delight.
- [ ] Global hotkeys (start/stop/pause), system tray, countdown + region-selector overlay window.
- [ ] Sensitivity controls, speed regions, click ripple / cursor highlight toggles.
- [ ] Code signing (Azure Trusted Signing), NSIS installer, auto-updater, `latest.json` hosting.

## Later (post-v1, architecture already supports)

Audio tracks (system + mic, separate); text/arrow annotations; webcam PiP; macOS/Linux builds;
cloud/sharing; AI features. Hold the [spec §7](./Vuoom-Spec.md) out-of-scope line for v1.

---

## Risk register (carry through every milestone)

| Risk | Milestone | Mitigation |
|---|---|---|
| Auto-zoom feels janky / motion-sick | M2 | Heavy easing/debounce/frequency tuning vs real recordings; the §5.1 gate |
| Preview bridge perf | M0/M3 | Decided up front (WebSocket); spike before the editor; URI-protocol fallback |
| Mixed-DPI / multi-monitor coord bugs | M1 | Test mixed-DPI from M1; everything in physical px |
| GIF too large | M4 | gifski + presets + sample-extrapolate size estimate |
| **gifski AGPL / codec licensing** | M4 | Resolve in [`10`](./10-Licensing.md) *before* shipping export |
| ffmpeg/gifski distribution | M4/M5 | Ship as sidecars / LGPL builds; document licenses |
| SmartScreen on free download | M5 | Azure Trusted Signing |
| Scope creep | all | Hold the spec §7 line |
