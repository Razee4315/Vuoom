# 07 — Competitive Landscape, OSS Reuse Map & Positioning

Where Vuoom fits, what to learn from existing tools, and exactly which open-source code is safe to
reuse vs. study-only.

---

## The market gap (the wedge)

**"Screen Studio for Windows, free and native."** It's a real, under-served gap:

- **Screen Studio** — the gold-standard auto-zoom recorder — is **macOS-only** and moved to
  subscription in 2025.
- The strongest **Windows** auto-zoom tools are **closed/paid** (FocuSee, Rapidemo) or
  **browser-extension-limited** (Cursorful).
- The only open, native, Windows-capable Tauri+Rust competitor is **Cap**, which is **AGPL**
  (off-putting to many; unusable as a base for a permissive tool) and only shipped auto-zoom in
  **Feb 2026 behind an experimental flag**.

→ A polished, **free, permissive, native-Windows** tool with **auto-zoom on by default** has clear
air.

## Comparison (condensed)

| Tool | Platform | Open? | Auto-zoom on click? | Stack | Note |
|---|---|---|---|---|---|
| **Screen Studio** | macOS | Closed | **Yes** (benchmark) | Native | The target to match; not on Windows |
| **Cap** (cap.so) | mac + **Win** | **AGPLv3** (+MIT subcrates) | Yes (Feb 2026, experimental) | **Tauri 2 + Rust + wgpu** | Closest analog; study architecture, don't copy AGPL |
| **FocuSee** | Win + Mac | Closed | Yes (AI track + zoom) | Native | Strongest closed Windows incumbent |
| **Cursorful** | Browser (+desktop) | Closed (free tier) | Yes (2+ clicks/3s heuristic) | Extension | Good zoom-trigger heuristic reference |
| **Rapidemo** | Windows | Closed | Yes | Native | Direct Windows SS clone |
| **Screenize** | macOS | **Apache-2.0** | Yes (spring physics + per-activity) | Swift | **Best UX/algorithm reference** (design only) |
| **screen-demo** (njraladdin) | **Win** + Mac | **MIT** | Yes | **Tauri + Rust + React** | **Most reusable** — MIT + our exact stack |
| **Recordly** | Win/Mac/Linux | **MIT** | Yes (suggests zoom regions) | Mixed | Zoom-region detection reference |
| **Cursorfly** | Browser | **Open** | Yes (zoom/pan at click) | Extension | Client-side zoom/pan reference |
| CleanShot X | macOS | Closed | No | Native | Click highlights only |
| Loom / Jumpshare / Screencastify | varies | Closed | No | web/native | Sharing-first, not cinematic |
| Kap | macOS | **MIT** | No | Electron-era | Plugin-architecture reference |
| Screenity | Chrome | Open | No | Extension | Privacy/in-browser editing |

## Open-source reuse map (what's *safe* vs *study-only*)

| Repo | License | Usage |
|---|---|---|
| **njraladdin/screen-demo** | **MIT** | ✅ **Reuse-friendly.** Closest Tauri+Rust+Windows reference; study zoom-animation timing (playhead-based) and cursor/multi-monitor code. |
| **CapSoftware/scap** | **MIT** | ✅ Reusable cross-platform capture crate (WIP). (We chose `windows-capture` instead, but scap is a fine reference.) |
| Cap `cap-camera*` subcrates | **MIT** | ✅ Reusable. |
| **Recordly** | **MIT** | ✅ Reference for auto-zoom region detection. |
| **wulkano/Kap** | **MIT** | ✅ Plugin/export-format architecture pattern. |
| **syi0808/screenize** | **Apache-2.0** | ⚠️ Permissive but **Swift** → *design* reference only (dual camera modes, per-activity zoom planning, editable keyframes UX). |
| **CapSoftware/Cap** (app code) | **AGPLv3** | ⛔ **Study only. Never copy into permissive Vuoom.** Read `crates/rendering/zoom.rs`, `frame_ws.rs`, `enc-mediafoundation/mft.rs` for *understanding*; reimplement clean. |

## Distilled lessons learned (from devs who built this)

**Auto-zoom UX**
- Don't zoom on every mouse move → "seizure-inducing." Trigger on click clusters (Cursorful: 2+
  clicks within ~3 s); hold while activity continues.
- Plan zoom *level* by activity type (typing vs clicking vs scrolling). — Screenize
- **Make every auto keyframe visible and editable on a timeline.** Users distrust a black box.
  This is also a concrete way to beat Screen Studio's imprecise long-timeline scrubbing.

**Rust + wgpu pipeline** (from a dev who built an SS alternative in Rust/wgpu)
- A wgpu pipeline is the right call — once the base works, zoom/cursor/background are *additive*.
- **Separate preview rendering from export** for UI responsiveness.
- **Export is the perennial bottleneck** — budget real time; GPU-accelerated encode gives big wins.

**Tauri / Electron-migration** (HN: "rewrote Electron app in Rust")
- Wins: ~83% smaller app, much faster, fewer background crashes.
- Costs: Tauri uses the **system webview** → rendering varies by OS/webview version; needs far
  more cross-platform testing; sidecar bundling can be painful (esp. macOS).
- **Vuoom's structural advantage: Windows-only.** We sidestep the worst of Tauri's cross-platform
  webview tax that burned the migrators — a real edge over cross-platform Cap.

## Positioning recommendation

**Table-stakes (to be credible):** reliable WGC capture (multi-monitor); auto-zoom that "just
works"; cursor smoothing + click ripples; backgrounds/padding; simple text labels; **small,
high-quality GIF export** (Vuoom's single output — own this).

**Differentiators (where Vuoom wins):**
- **Free + permissive + native Windows** — no strong auto-zoom tool combines all three.
- **Auto-zoom on by default** with **editable keyframes on a visible timeline** (Screenize's
  model) — beat the black-box feel.
- **Lean, fast exports** — directly attack Screen Studio's pain (a 12-min 4K export ≈ 9 GB).
- **No login, 100% local, no watermark** — privacy + zero friction.

**One-line positioning:** *Vuoom is the free, native Windows screen recorder that automatically
makes your demos look cinematic — auto-zoom on by default, exports tiny.*

## Sources

Cap: <https://github.com/CapSoftware/Cap> (issue #352 auto-zoom demand) · Screen Studio review:
<https://scribehow.com/page/Screen_Studio_Review_2026_I_Tested_the_Auto-Zoom_Mac_Recorder_for_90_Days__Heres_the_Truth__0R7wu5TiSvqYAK3TzdygdQ> ·
screen-demo: <https://github.com/njraladdin/screen-demo> · screenize:
<https://github.com/syi0808/screenize> · Recordly:
<https://abduzeedo.com/recordly-free-open-source-screen-recorder-auto-zoom> · Cursorful:
<https://cursorful.com/> · Kap: <https://github.com/wulkano/Kap> · Rust/wgpu lessons:
<https://dev.to/mathisdev7/im-19-and-i-built-a-screen-studio-alternative-for-linux-with-rust-and-wgpu-heres-what-i-learned-log> ·
Tauri migration: <https://news.ycombinator.com/item?id=44118023> · AGPL:
<https://choosealicense.com/licenses/agpl-3.0/>
