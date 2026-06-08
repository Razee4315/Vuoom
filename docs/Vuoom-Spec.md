# Vuoom — Product & Technical Specification

> **Vuoom** — screen recordings that zoom where it matters.
> A free, lightweight, cinematic screen recorder that auto-zooms into your cursor and exports clean MP4/GIF product demos. Built with Tauri + Rust.

**Document status:** v1 spec / build brief · **amended v1.1** (see box below)
**Primary platform:** Windows 10/11 (cross-platform-ready architecture)
**Intended reader:** the implementing developer (you or a contractor)

---

> ## ⚠️ Scope amendments (v1.1 — 2026-06-08) — these OVERRIDE the sections below
>
> Owner decisions that refine v1. Where this box conflicts with later sections, **this box wins.**
>
> 1. **GIF-only output.** v1 exports **GIF only** (via gifski). **MP4/H.264/H.265 are dropped
>    from v1** — ignore all MP4/codec content in §5.3, §6 (Export), §9.1, §9.5. The architecture
>    still leaves room to add MP4 later, but it is out of v1. This also removes the entire video
>    codec patent/licensing problem. Product framing: *the simplest way for programmers to make a
>    demo GIF for a README.*
> 2. **No audio.** Drop system/mic audio entirely from v1 (GIFs have no audio). Ignore audio
>    mentions in §5.2 and §6.
> 3. **Text annotations are a CORE v1 feature** (not "phased"). Plus basic **arrow** and
>    **highlight box** annotations. Must be **simple and easy** to add/edit. See
>    [`11-Editor-and-Annotations.md`](./11-Editor-and-Annotations.md).
> 4. **The editing UI is a first-class priority** — clean, simple, "looks good by default."
> 5. **License = Apache-2.0** (resolves §13 / open question). gifski stays isolated as an
>    out-of-process binary so Vuoom's code remains Apache-2.0. See
>    [`10-Licensing.md`](./10-Licensing.md).
>
> Everything else in this spec (auto-zoom as the heart of the product, native Tauri+Rust+wgpu,
> capture quality, framing/backgrounds, free & no-friction) stands unchanged.

---

## 1. Elevator pitch

Most screen recorders give you a flat, raw capture of your whole screen. The result is boring to watch and forces the viewer to hunt for where the action is. The tools that *do* solve this — the ones that smoothly zoom into each click like a little camera operator following your cursor — are almost all Mac-only, paid, or subscription-locked.

Vuoom brings that "Screen Studio" cinematic auto-zoom experience to Windows, for free, in a small native app. You record, Vuoom automatically zooms into wherever you click and pans smoothly between actions, and you export a polished MP4 or GIF ready for a GitHub README, a Reddit post, a product page, or social media.

---

## 2. Why I'm building this (motivation)

- **The good tools are locked away.** The signature auto-zoom-on-click effect was popularized by Screen Studio, which is macOS-only and expensive. Windows users — who are the majority of developers — have no clean, free, native equivalent.
- **Raw recordings look amateur.** A flat 1080p capture of an entire 4K monitor makes UI text tiny and the "wow" moment invisible. Manually adding zoom keyframes in a video editor (Kdenlive, DaVinci, Camtasia) is tedious and most people just don't bother.
- **Existing "free" options compromise on quality.** Many cross-platform alternatives capture through a browser/Electron pipeline, which limits capture quality, cursor fidelity, and bloats memory. A native Rust pipeline can do better while staying lightweight.
- **Developers ship demos constantly.** Every repo README, every "I built this" post, every changelog entry benefits from a short, sharp product GIF. The current workflow (record → import to editor → manually animate → export → optimize GIF) is far too heavy for a 10-second clip.

**In one line:** turning a raw screen capture into a professional product demo should take seconds, be free, run natively on Windows, and not require any video-editing skill.

---

## 3. The problem we're solving

| Pain | Today | With Vuoom |
|---|---|---|
| Action is hard to follow in a recording | Viewer squints at a full-screen capture | Camera auto-zooms into each click |
| Making it cinematic | Manual keyframing in a video editor | Automatic, with optional manual tweaks |
| The good tool is Mac-only / paid | Windows users left out | Free, native Windows app |
| Heavyweight tools | Electron bloat, high RAM | Small Rust + Tauri footprint |
| GIFs look bad / are huge | Hand-tuning ffmpeg/gifsicle | One-click optimized GIF export |
| Output for different platforms | Re-export manually per aspect ratio | Presets for 16:9 / 9:16 / 1:1 |

---

## 4. Target users

- **Primary:** indie developers and open-source maintainers who need quick, good-looking demo GIFs/videos for READMEs, Reddit, X, Product Hunt, and changelogs.
- **Secondary:** SaaS founders, technical writers, support teams making how-to clips, and content creators who want cinematic screen demos without a video editor.

---

## 5. Non-negotiable core features (no compromise)

These define the product. If any of these is weak, Vuoom is just another screen recorder. **Build quality here above everything else.**

### 5.1 Auto-zoom that follows the cursor — the heart of the product

This is *the* feature. It must look as good as the best paid tools.

**Required behavior:**
- Automatically detect "moments of interest" during recording — primarily **mouse clicks**, but also drag starts, scroll bursts, and sustained keyboard input — and zoom the virtual camera into the cursor location at that moment.
- Zoom transitions must be **smooth and eased** (ease-in/ease-out, ideally a subtle spring), never a hard cut or linear jump.
- After a configurable period of inactivity, the camera **smoothly pulls back out** to the full frame.
- The camera **pans smoothly** between consecutive zoom targets rather than snapping.
- Cursor jitter must not trigger zooms — use a **dead zone / debounce** so only meaningful actions cause camera movement.
- The cursor must **always stay inside the visible frame** during a zoom; the camera clamps so it never shows off-screen/empty areas.
- Limit zoom frequency (e.g., at most one camera move per ~1.2–1.5s by default) so the result never feels like motion sickness.

**Must be configurable (with good defaults):**
- Zoom depth (e.g., 1.5×–3×)
- Hold duration before pull-back
- Easing curve / "springiness"
- Sensitivity (what counts as a trigger)
- Per-region overrides

**Manual control (must exist, layered on top of auto):**
- Every auto-placed zoom appears as an editable block on a timeline.
- The user can add, remove, move, resize, and re-target zoom regions **without re-recording**.
- The user can set the zoom focus point precisely (pixel-level).

**Acceptance criteria for this feature (definition of "done"):**
1. Camera movement renders smoothly at 60 fps with no visible stutter or tearing.
2. With default settings, a typical "click around a UI" recording produces zooms that a non-expert would describe as "professional / like Screen Studio."
3. No zoom ever shows empty space outside the captured content.
4. Auto-placed zooms are fully editable after recording.
5. Tiny cursor movements and accidental micro-clicks do not cause camera jumps.

### 5.2 High-quality native capture
- Capture a **full display**, a **single window**, or a **custom region**.
- Capture at up to **60 fps** and full monitor resolution (incl. 4K and high-DPI).
- Use the **native Windows capture pipeline** (Windows Graphics Capture) for clean, performant capture — not a browser/Electron capture path.
- Record **system audio and microphone** on separate tracks (audio can ship in a later phase, but architect for it now).

### 5.3 Clean export to MP4 and GIF
- Export **MP4** (H.264 by default, H.265 optional) and **GIF**.
- **Optimized GIF output** — small file size with good quality (this is its own hard problem; see §9.5). Never produce a 40 MB README GIF by default.
- **Aspect-ratio presets:** 16:9 (YouTube/README), 9:16 (Shorts/Reels), 1:1 (square social).
- Resolution and quality presets, with a visible estimated output file size.

### 5.4 Framing / "make it look designed"
- Place the recording inside a **styled frame**: padding, background (solid color, gradient, or image/wallpaper), rounded corners, and drop shadow.
- These are what make a demo look intentional rather than captured.

### 5.5 Lightweight & native
- Built on **Tauri + Rust**, not Electron. The app should be small to download and modest in RAM/CPU at idle.
- Fast cold start; recording should begin within ~1–2 seconds of pressing record.

### 5.6 Free and no-friction
- No account, no login, no watermark, no paywall on core features.
- Works fully offline. No telemetry by default (if any analytics are added later, opt-in only).

---

## 6. Full feature list (organized)

**Capture**
- Display / window / region capture
- Multi-monitor support and selection
- High-DPI and 4K aware
- Up to 60 fps
- System + mic audio on separate tracks (phased)
- Live cursor position + click event capture (drives auto-zoom)

**Zoom & motion**
- Automatic zoom-on-action (see §5.1)
- Manual zoom region editing on a timeline
- Configurable depth / hold / easing / sensitivity
- Smooth pan between targets
- Cursor smoothing and optional cursor size/highlight effects
- Click ripple / highlight effect (optional toggle)

**Editor**
- Timeline with scrubbing and frame-accurate preview
- Trim and cut
- Per-segment speed control (speed up dead time, slow down key moments)
- Add/edit/remove zoom regions
- Background, padding, corner radius, shadow controls
- Aspect-ratio switcher with live reframe
- Optional text/arrow annotations (phased)
- Save/reopen as a project file

**Export**
- MP4 (H.264 / H.265)
- Optimized GIF
- Aspect-ratio + resolution + quality presets
- Estimated file size before export
- Quick "copy GIF to clipboard" for instant pasting

**UX**
- Global hotkeys (start/stop/pause)
- System tray presence
- Drag-and-drop nothing required; record → edit → export in one window
- Sensible defaults so a first-time user gets a great result with zero configuration

---

## 7. Explicitly out of scope for v1 (keep focus)

Deferring these protects the timeline and the quality of the core. Architect so they can be added later, but do **not** build them in v1:

- Webcam overlay / picture-in-picture
- Cloud upload, sharing links, hosting, analytics
- Team features / collaboration
- AI features (auto-captions, filler-word removal, etc.)
- macOS and Linux builds (architecture stays cross-platform; only Windows ships first)
- Live streaming
- A plugin/extension system

---

## 8. User flow

1. **Launch** → small launcher window: choose source (display / window / region), pick aspect ratio, set hotkeys.
2. **Record** → press hotkey, a countdown, then recording. Vuoom silently logs cursor position + click timestamps alongside the video.
3. **Stop** → drops straight into the editor.
4. **Auto-magic** → zoom regions are already placed based on the click log; a styled background is already applied with defaults.
5. **Tweak (optional)** → adjust zooms, trim, change background/aspect, speed up dead spots.
6. **Export** → pick MP4 or GIF + preset → file saved, or GIF copied to clipboard.

The promise: steps 1–3 + 6 alone (skipping 5 entirely) should already produce something you're happy to post.

---

## 9. Technical architecture

> Core principle: **the webview is the cockpit, Rust is the engine.** All heavy lifting — capture, compositing, encoding — happens in native Rust. The Tauri webview renders only the UI (launcher, timeline, controls). Do **not** try to capture or composite video inside the webview; that path leads to the same quality ceiling as Electron tools.

### 9.1 Stack
- **Shell / UI:** Tauri 2.x. Frontend in TypeScript with a lightweight framework (React/Svelte/Solid — implementer's choice) + Tailwind for the editor UI.
- **Core engine:** Rust.
- **Capture:** `scap` (uses Windows Graphics Capture on Windows; cross-platform). Alternative to evaluate: `CrabGrab`, which integrates directly with `wgpu`/DXGI and may simplify the capture→GPU path. Evaluate both early.
- **Input tracking:** `windows-rs` low-level hooks (`WH_MOUSE_LL`, `WH_KEYBOARD_LL`) or Raw Input to log global cursor position, clicks, and key events **with timestamps** synchronized to capture frames. This event log is what powers auto-zoom.
- **GPU compositing:** `wgpu`. Shaders apply the zoom/pan transform (scale + translate with eased keyframes), background, padding, rounded corners, shadow, and optional motion blur.
- **Encoding:** `ffmpeg` (shipped as a Tauri sidecar binary) for MP4 (H.264/H.265). `gifski` (Rust crate) for high-quality GIF; optionally post-process with `gifsicle` for size.
- **Project storage:** a JSON manifest (`.vuoom`) describing edits + a reference to the captured intermediate video.

### 9.2 Pipeline overview

```
[Windows Graphics Capture]  +  [Global input hook: clicks/keys/cursor + timestamps]
            |                                   |
            v                                   v
   Captured frames (BGRA)              Event log (drives auto-zoom keyframes)
            |                                   |
            +-----------------+-----------------+
                              v
                Auto-zoom planner (generates camera keyframes)
                              v
            wgpu compositor (zoom/pan + background + frame styling)
                    |                         |
                    v                         v
          Live preview (to webview)     Final render pass
                                              |
                                   +----------+----------+
                                   v                     v
                            ffmpeg → MP4           gifski → GIF
```

### 9.3 Auto-zoom planner (the key algorithm)
- Input: the timestamped event log + the recording duration.
- Output: a list of **camera keyframes** (time, center point, zoom level, easing).
- Logic:
  - Cluster click/activity events; ignore events within the dead-zone/debounce window.
  - For each cluster, create a zoom-in keyframe targeting the smoothed cursor position, hold for the configured duration, then a zoom-out keyframe after inactivity.
  - Smooth the camera *path* (e.g., Catmull-Rom or critically-damped spring) so motion between targets is fluid.
  - Clamp the camera so the viewport never exits the captured content bounds.
- These keyframes are editable; the editor mutates this same data structure.

### 9.4 The hard problem — preview frame bridge (decide this first)
Raw frames are huge: a 1080p BGRA frame is ~8 MB; at 60 fps that's ~500 MB/s. **You cannot send raw frames over Tauri's command IPC** (it serializes them — it will choke). Pick one approach before building the editor, because retrofitting is painful:

- **Option A — native preview surface:** render the wgpu preview into a separate native child window layered into the Tauri window. Highest performance, more platform-specific window wrangling.
- **Option B — Tauri custom protocol:** stream encoded/low-latency preview frames through a custom protocol (more efficient than the JSON command bridge) into a `<canvas>`/`<video>`.
- **Option C — shared GPU texture / shared memory** between the Rust render and the webview.

Recommended: prototype **Option A** first; it gives the cleanest 60 fps scrubbing. Keep the final *export* render fully in Rust regardless.

### 9.5 GIF quality (its own requirement)
- Use `gifski` for perceptually good palettes; cap default output (e.g., ~15 fps, width ~1000px) and show the estimated size.
- Provide a "README GIF" preset (small, autoplays well) and a "high quality" preset.
- Offer "copy GIF to clipboard" — the single most-used action for posting.

### 9.6 Project file format (`.vuoom`)
- A JSON manifest: source metadata, capture file reference, camera keyframes, trims, speed regions, background/frame settings, aspect ratio, annotations.
- Store the captured video as a near-lossless intermediate so re-edits/re-exports don't degrade quality.
- Re-opening a project restores the full editable state.

---

## 10. Performance & quality targets

- Capture: sustained 60 fps at 1080p; graceful handling of 4K (may target 30 fps at 4K initially).
- Preview: smooth scrubbing without dropped frames on a mid-range machine.
- Idle RAM: small (Tauri-class, not Electron-class).
- Recording start latency: ≤ ~2 s after hotkey.
- Export: a 30 s clip should export to MP4 in well under real-time on a typical GPU.
- Visual bar: the auto-zoom output should be indistinguishable in polish from leading paid tools to a non-expert.

---

## 11. Suggested build phases (milestones)

1. **M1 — Capture core:** display/window/region capture via `scap`, write to disk, basic playback. Prove 60 fps capture works.
2. **M2 — Input log + auto-zoom planner:** global click/cursor logging synced to frames; generate camera keyframes; apply a basic wgpu zoom/pan transform on export. *This is the make-or-break milestone.*
3. **M3 — Editor + preview bridge:** timeline, scrubbing preview (solve §9.4), edit zoom regions, trim.
4. **M4 — Framing & export:** backgrounds/padding/corners/shadow; MP4 + optimized GIF; aspect presets; copy-to-clipboard.
5. **M5 — Polish:** defaults tuning, hotkeys, tray, sensitivity, speed regions, click effects.
6. **Later:** audio tracks, annotations, webcam, other platforms.

Ship M1–M4 as the first public release; M2's quality is the gate.

---

## 12. Risks & mitigations

| Risk | Mitigation |
|---|---|
| Auto-zoom feels janky / motion-sick | Invest heavily in easing, debounce, frequency limits; tune defaults against real recordings |
| Preview frame bridge performance | Decide architecture (§9.4) before building the editor; prototype early |
| High-DPI / multi-monitor coordinate bugs | Test on mixed-DPI multi-monitor setups from M1 |
| GIF files too large | gifski + presets + size estimate + sane defaults |
| Scope creep | Hold the §7 out-of-scope line for v1 |
| ffmpeg/gifski binary distribution | Ship as Tauri sidecars; document licensing |

---

## 13. Naming & branding

- **Name:** **Vuoom** — reads like "vroom" (fast motion) crossed with the double-o of "zoom"; directly evokes the punchy zoom-in that is the product's core.
- Verified clear of existing software products, GitHub repos, and app-store listings at time of writing (only incidental non-software usage exists). **Reserve `github.com/vuoom`, a `vuoom.app`/`vuoom.dev` domain, and social handles before launch.**
- **Tagline:** "Screen recordings that zoom where it matters."
- **Tone:** developer-first, no-nonsense, free-and-native. Avoid corporate/AI-marketing voice in launch copy.

---

## 14. Open questions for the implementer

1. Frontend framework choice for the editor (React / Svelte / Solid)?
2. Capture crate: `scap` vs `CrabGrab` after a spike — which gives the cleaner wgpu path?
3. Audio in v1 or deferred to a later milestone?
4. Minimum supported Windows version (10 1809+? 11 only for cleanest WGC behavior)?
5. Preview bridge: confirm Option A (native surface) after prototyping.

---

*End of spec. This document is the single source of truth for v1; changes to scope should be reflected here.*
