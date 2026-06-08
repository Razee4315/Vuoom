# 09 — Decision Records & Open Questions

Lightweight ADRs for the decisions already made, plus the open questions that need a human/owner
call. Update this file as decisions land.

---

## Accepted decisions (ADRs)

### ADR-001 — Capture via `windows-capture` (WGC), GPU-resident
- **Decision:** Use `windows-capture` 2.0 as the primary capture layer; DXGI Desktop Duplication
  as fallback; keep frames as D3D11 textures.
- **Why:** Only mature Rust WGC wrapper that exposes the GPU texture + cursor exclusion + QPC
  timestamps; proven by Cap. `scap`/`CrabGrab` rejected (CPU-only / minimal maintenance).
- **Detail:** [`03`](./03-Capture.md).

### ADR-002 — One QPC clock for input + video
- **Decision:** QueryPerformanceCounter everywhere; align input events to WGC `SystemRelativeTime`.
- **Why:** Frame-accurate auto-zoom needs a single shared time axis; `MSLLHOOKSTRUCT.time` is too
  coarse. **Detail:** [`02`](./02-Architecture.md), [`04`](./04-Input-and-AutoZoom.md).

### ADR-003 — Input via Raw Input, not low-level hooks
- **Decision:** `windows-rs` Raw Input (`RIDEV_INPUTSINK`) on a dedicated thread; `rdev` for
  prototype only.
- **Why:** Microsoft-recommended; off the input critical path; lower AV-flagging risk.
- **Detail:** [`04`](./04-Input-and-AutoZoom.md).

### ADR-004 — Auto-zoom via critically-damped springs
- **Decision:** Click-driven planner + half-life-parameterized critically-damped springs +
  off-screen clamp + dead-zone. Defaults documented.
- **Why:** Self-correcting, no overshoot, frame-rate independent; matches Cap's approach.
- **Detail:** [`04`](./04-Input-and-AutoZoom.md).

### ADR-005 — wgpu on DX12, one compositor, two sinks
- **Decision:** Single wgpu device (DX12 backend), offscreen render, shared by preview and export.
- **Why:** Stability/perf on Windows; guarantees preview == export.
- **Detail:** [`05`](./05-Compositing-and-Preview.md).

### ADR-006 — Preview bridge = localhost binary WebSocket
- **Decision:** Offscreen RGBA readback → `127.0.0.1` WebSocket → Web Worker → WebGPU canvas;
  scrubbing is a cheap `seekTo` command. Async custom-URI-protocol is the fallback.
- **Why:** Pixels can't cross JSON IPC; proven by Cap at 60fps; Option A (native overlay) is
  fragile on Windows and Option C (shared texture) is impossible on WebView2.
- **Detail:** [`05`](./05-Compositing-and-Preview.md).

### ADR-007 — ~~MP4 via Media Foundation HW encoder~~ **(SUPERSEDED by ADR-012 — MP4 is out of v1)**
- **Original decision (preserved for if MP4 returns):** Native Media Foundation HW encoder primary;
  `ffmpeg-next` HW fallback; ship no GPL codecs. Copyleft-safe **and** patent-safe; mirrors Cap.
- **Status:** Not applicable to v1 (GIF-only). Kept here so the chosen MP4 path is on record for a
  future version. **Detail:** [`06`](./06-Export.md), [`10`](./10-Licensing.md).

### ADR-008 — GIF via gifski, process-isolated
- **Decision:** Use gifski for quality; isolate the AGPL obligation (see open question OQ-2).
- **Detail:** [`06`](./06-Export.md), [`10`](./10-Licensing.md).

### ADR-009 — Frontend = SolidJS + Vite + Tailwind; canvas timeline
- **Decision:** SolidJS (fine-grained reactivity for 60fps scrub); render timeline on `<canvas>`.
- **Why:** No VDOM diff on the hot scrub path; Svelte 5 is the fallback.
- **Detail:** [`02`](./02-Architecture.md).

### ADR-010 — Rust→JS streaming via `tauri::ipc::Channel`
- **Decision:** Channels (not events) for encode/recording progress and preview signaling.
- **Why:** Ordered, cheap, purpose-built; events are for small fire-and-forget only.

### ADR-011 — License = Apache-2.0 (resolves former OQ-1)
- **Decision:** All of Vuoom's own code is **Apache-2.0**.
- **Why:** Permissive + explicit patent grant + contribution/patent-retaliation clarity. `LICENSE`
  and `NOTICE` added to the repo. **Detail:** [`10`](./10-Licensing.md).

### ADR-012 — v1 is GIF-only; no MP4; no audio (resolves former OQ-3)
- **Decision:** v1 exports **GIF only** (gifski). MP4/H.264/H.265 and audio are dropped from v1.
- **Why:** Sharper product (programmer demo GIFs); erases the entire video-codec patent/licensing
  minefield; smaller surface area. MP4 stays a clean future add (compositor already emits RGBA).
- **Detail:** [`06`](./06-Export.md), [spec v1.1 amendments](./Vuoom-Spec.md).

### ADR-013 — gifski shipped out-of-process (resolves former OQ-2)
- **Decision:** Ship gifski as a **separate, unmodified binary, invoked out-of-process** (mere
  aggregation) — never linked.
- **Why:** Keeps Vuoom's source Apache-2.0 despite gifski being AGPL. (Get legal sign-off.)
- **Detail:** [`10`](./10-Licensing.md).

### ADR-014 — Text + basic annotations are core v1; rendered via glyphon + lyon
- **Decision:** Text labels (glyphon), arrows + highlight boxes (lyon), optional spotlight/blur,
  all drawn in the wgpu pass; animation driven by CPU timeline state. Editor stays simple.
- **Why:** Owner priority; permissive licenses; integrates without a separate render pass.
- **Detail:** [`11`](./11-Editor-and-Annotations.md), [`05`](./05-Compositing-and-Preview.md).

---

## Open questions (need an owner decision)

> Resolved: OQ-1 (license → Apache-2.0, ADR-011) · OQ-2 (gifski → out-of-process, ADR-013) ·
> OQ-3 (audio → dropped from v1, ADR-012). Remaining below.

### OQ-4 — Minimum Windows version: Win10 1809+ or Win11-only?
- **Impact:** Borderless capture (no yellow border) and WGC dirty-region need **Win11**; Win10
  loses those (DXGI fallback gives borderless full-display but not per-window).
- **Recommendation:** Support **Win10 1809+** with graceful degradation (yellow border on Win10
  window capture; borderless via DXGI for full-display); make Win11 the "best experience."

### OQ-5 — 4K preview strategy
- **Impact:** Raw RGBA over WebSocket is heavy at 4K60 (~2 GB/s).
- **Recommendation:** half-res preview at 4K by default; offer H.264+WebCodecs preview only if a
  user needs full-res 4K scrubbing. Decide the auto-switch threshold during M3.

### OQ-6 — Reserve naming/handles
- **Action (from spec §13):** reserve `github.com/vuoom`, a `vuoom.app`/`vuoom.dev` domain, and
  social handles before any public mention. Not a technical decision but a launch blocker.

---

## Decision-making notes

- Prefer reversible decisions made fast; flag irreversible ones (license, codec strategy) for the
  owner.
- When a decision depends on real-hardware behavior (preview perf, encoder availability), resolve
  it with an M0 spike, not a debate.
