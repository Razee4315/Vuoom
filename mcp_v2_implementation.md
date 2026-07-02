# AI Demo Director v2 — Implementation Summary

**Branch:** `feat/ai-demo-director-mcp` (PR #1) · **Date:** 2026-07-02 · **CI:** ✅ green
(rustfmt, clippy `-D warnings`, all tests, full workspace check)

This implements the complete improvement plan from [`mcp_improve.md`](mcp_improve.md) —
the review that found why agent recordings looked robotic and why the critique loop was
half-blind. Four commits, each pushed green.

---

## Commit 1 — Rebase onto latest main (`266c517`)

Picked up ~20 new main commits (v0.1.28–v0.1.30: streaming GIF encoder, binary-search
frame lookup, poison-resilient locks, UI fixes). The textual merge missed **two semantic
conflicts** that had to be fixed by hand:

- main's locks became poison-resilient (`.lock().unwrap_or_else(|e| e.into_inner())`) —
  the branch's control code still used `map_err(|_| "lock poisoned")?`
- `nearest_idx` now takes `store.recs()` (a `&[FrameRec]`), not the store itself

`Cargo.lock` was regenerated minimally via `cargo metadata` (no full re-resolution).

> Lesson for future rebases of this branch: those two patterns are invisible to git's
> merge — always grep for them after rebasing.

## Commit 2 — The humanizer (`4e8d2ee`) — *cursor flow fully smooth*

The root cause of "robotic" recordings: the hardware cursor is baked into the captured
pixels (WGC draws it), and every injected action warped it instantly. Fixed at the
source, in `crates/vuoom-input/src/inject.rs`:

| Before | Now |
|---|---|
| Click teleports the cursor in one `SendInput` batch | **Minimum-jerk glide** (`10p³−15p⁴+6p⁵`, zero end velocity/acceleration) at ~125 Hz, duration scaled to distance (`clamp(150 + px/3, 200, 900)` ms) → settle ~120 ms → press 60 ms → release |
| Whole string typed in ~0 ms | **Paced typing** ~15 cps (0.5–200 configurable) with deterministic per-char jitter; `\n`/`\t` press real Enter/Tab |
| Scroll = one violent jump | **One notch per 40 ms step** so smooth-scrolling apps animate |
| No drag at all | **`drag`**: glide → hold → eased path → release (sliders, drag-drop, selection) |
| `SendInput` failures swallowed | Short count → `Err` ("is the target app running elevated?") — UIPI can no longer produce a demo of nothing happening |
| Arrows read as numpad keys in scan-code apps | `KEYEVENTF_EXTENDEDKEY` for the nav cluster |
| No punctuation chords | OEM keys in `key_to_vk`: `- = [ ] ; ' , . / \ ` `` → `ctrl+=` browser zoom works |

Double win: the glide streams **real move events through the low-level hook**, so the
auto-zoom camera follows a smooth path instead of a step function — cursor AND camera
smoothness had the same root cause. All pure math (`min_jerk`, `glide_points`,
`glide_duration_ms`, `jitter_factor`, `is_extended_vk`) is unit-tested.

## Commit 3 — Protocol v2 across the stack (`66b0992`) — *the loop can see and repair*

**Perception** (the agent was driving blind):
- `screenshot` — live PNG of the recording monitor, any time (reuses
  `Session::screenshot`'s capture path, downscalable)
- `clip_state` now returns the actual **zoom/cut/speed spans** (`{start, end, amount,
  focus}`) plus `output_duration` — not bare counts, so the agent knows exactly where to
  sample and which index to repair
- `get_frames` / `seek` sample the **output timeline**, mapped through trim + cuts +
  speed via the same `out_mapping`/`output_to_source` path export uses — the agent
  critiques *exactly what ships*

**Repair** (before: the only fix was a full re-record):
- `add_zoom` / `update_zoom` / `set_zoom_focus` / `remove_zoom`
- `add_cut` / `update_cut` / `remove_cut`
- `auto_speed` / `clear_speed` / `set_trim`
- `set_zoom_style` (hold, pre-roll, spring half-lives — per-recording)
- `status` (`idle|recording|paused|clip_ready` + elapsed) and `cancel_recording`

**Hardening:**
- **Auth token**: random 128-bit token in `%TEMP%\vuoom-control.json`, required as the
  first line of every connection — loopback is not a trust boundary for input injection
- Client **connect (3 s) / read (120 s) timeouts** — a hung server can't wedge the agent
- **Exports are jobs**: `export_gif`/`export_mp4` return an id immediately;
  `export_status` polls `{done, total, finished, error, path}`
- `VUOOM_ENABLE_CONTROL=0`/`false`/`off` now actually disables; discovery file deleted
  on app exit (`RunEvent::Exit`)
- Agent-scoped flags (`auto_zoom_on_click`, zoom style) **reset at stop/cancel/failed
  start** — an agent take can never change how your next interactive recording behaves

**MCP sidecar polish:** 35 tools total; `get_frames` capped at 16 frames with an 800 px
default width (protects the agent's context); `wait` clamped to 15 s; protocol-shape
mismatches are errors instead of parseable-looking text; the connect instructions teach
the full rhythm — *see → drive → settle 800–1500 ms → tail-wait 2500 ms → critique →
repair → export, cap at 3–4 takes*.

## Commit 4 — Tests + docs (`5c0dca3`)

- `vuoom-control/tests/client_auth.rs` — authenticated round trip works; wrong token is
  dropped with a clear "closed the connection" error
- `vuoom-mcp` router smoke test — asserts all 35 director tools are registered (the
  `#[tool_handler]` macro wiring is otherwise invisible until an agent connects)
- Protocol round-trip tests extended to every new variant + optional-field defaults
- Mock server upgraded to protocol v2 (token handshake + all ops)
- `docs/AI_DEMO_DIRECTOR.md` rewritten; `docs/IMPLEMENTATION-STATUS.md` gained the v2
  table; `mcp_improve.md` marked implemented

---

## What's still open

1. **Runtime pass on the real machine** (CI is headless — SendInput/GPU can't run there):
   ```powershell
   cargo build --release -p vuoom-mcp
   $env:VUOOM_ENABLE_CONTROL = "1"; .\Vuoom.exe   # or pnpm tauri dev
   ```
   then let an agent drive a full take and check: cursor glides (no teleport), typing
   pace, zoom centring, `get_frames` matching the export, export job completing.
2. **v2 endgame — synthetic cursor rendering** (`mcp_improve.md` §1.1 / roadmap 19):
   capture with `CursorCaptureSettings::WithoutCursor` and draw a smoothed cursor in the
   compositor. Decouples the pointer from OS jitter entirely, allows cursor scaling under
   zoom, and improves *human* recordings too. The Screen-Studio-grade final polish.
3. Two consciously skipped items: server-side `min_action_gap_ms` (rhythm lives in the
   MCP instructions instead) and single-connection enforcement (documented, accepted).
