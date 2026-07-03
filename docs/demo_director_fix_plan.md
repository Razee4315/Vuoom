# AI Demo Director — Verified Issues & Fix Plan

**Date:** 2026-07-03
**Input:** `demo-output/issues.md` (critique of the 2026-07-02 demo run)
**Method:** every technical claim in that report was independently verified against the code
(file:line evidence below). Nothing was accepted on faith. Craft/directing observations
(sections 3, 4, 6 of the report) are process guidance, not code bugs, and are out of scope here
except where a missing feature blocks them.

---

## 1. Claim-by-claim verdicts

| # | Claim (issues.md) | Verdict | Root cause / evidence |
|---|---|---|---|
| 1 | `key_chord ["+"]` errors; calc showed 789 | **CONFIRMED** | `key_to_vk` (`crates/vuoom-input/src/inject.rs:124-161`) has no `"+"` entry; fallthrough `single_key_vk` (`:164-175`) rejects it → `control_server.rs:289-291` returns `unknown key: +` (hard error, not silent). Digits/enter resolved, each `+` call errored → keystrokes reaching calc were `7 8 9 Enter` = 789. **Latent bug:** `"plus"`/`"="` map to `VK_OEM_PLUS` (0xBB) which types `=` unshifted and `key_chord` never synthesizes Shift — so today NO key name can type a literal `+`. |
| 2 | Camera tracks the pointer, not the point of change; no content-rect focus | **CONFIRMED** | `ZoomMode` is only `Auto` (spring-smoothed cursor follow, `camera.rs:174-184`) or `Manual { pos: DVec2 }` — one fixed normalized point (`keyframe.rs:11-17`). No rect/bbox variant exists. `set_zoom_focus` accepts a point only (`session.rs:1429-1447`). |
| 3 | 4 clicks merged into one ~6.5 s wandering zoom | **CONFIRMED** | Clustering: click joins previous cluster if gap ≤ `merge_gap` 0.8 s AND distance ≤ `merge_radius` 0.15 (`planner.rs:96-111`); each click extends `end` by `hold` 1.8 s (`:116-140`); merged span stays `Auto` (`:142-148`) so focus pans across every click point — that IS the wander. 4 clicks over ~2 s + 1.8 hold + 0.3 pre_roll ≈ 6.5 s. Merge knobs are in `ZoomConfig` (`config.rs:41-58`) but NOT exposed via `set_zoom_style` (only hold/pre_roll/hl_zoom/hl_pan, `session.rs:173-182`) → recompile-only today. |
| 4 | No shot arc / no hold-on-reveal / uniform easing | **CONFIRMED (nuance)** | Envelope is a global critically-damped spring in/out (`camera.rs:32-43,193-197`); release only 0.85× faster; no per-span attack/hold/release or easing. BUT `update_zoom(index,start,end,amount)` + `set_zoom_focus` + `set_zoom_style` already allow retiming/pinning — the last run under-used them. |
| 5 | Framing ignores content bounds; no pan-to-caret | **CONFIRMED** | Viewport = uniform centered crop of side `1/zoom` (`vuoom-render/src/layout.rs:41-52`); only clamp is to frame edges (`camera.rs:50-58`), never content. `KeyType` events carry **no position** (`event.rs:36-37`) so typing can sustain a hold but can never move focus — caret motion is invisible to the camera. |
| 6 | Repair loop half-blind | **CONFIRMED ×2** | (a) `clip_state` returns `focus: None` for every Auto span (`session.rs:1031-1034`) — agent cannot see where the camera actually pointed, though a `CameraTrack` is already computed internally (`camera.rs:81`). (b) `get_frames` = PNG stills, max 16 (`session.rs:1064-1114`, `main.rs:29,636`); the live animated compositor (`crates/vuoom-preview`) is a WebSocket wired to the in-app webview only — no motion preview reaches the agent short of a full export. |
| 7 | Capture is a region, not a window | **CONFIRMED** | WGC **monitor** capture only (`capture.rs:151-179`, `pick_monitor :133-144`); `CropRegion` is a fixed-pixel crop of each monitor frame (`:100-125`) → grabs whatever is topmost. The `windows-capture` crate supports `Window` targets but it's never imported; repo has zero window-enumeration/GetWindowRect helpers (`windows_ext.rs` has only capture-exclusion + clipboard). |
| 8 | `auto_speed` needs ~5 s idle | **CONFIRMED (refined)** | Hardcoded in `session.rs:1225-1260`: `MIN_GAP=2.5`, `LEAD=0.6`, `TAIL=0.4`, post-trim region must be > 0.5 s. Real cutoff = 2.5 s idle (report guessed ~2 s); only `factor` (1.5–16) is an API param. |
| 9 | Stale sidecar vs v2 auth | **CONFIRMED, ALREADY REMEDIATED** | Auth handshake: first line must equal the token from `%TEMP%/vuoom-control.json` or the server drops the connection (`control_server.rs:93-104`; auth landed in `66b0992`, 2026-07-02). At demo time `.mcp.json`'s target `target/release/vuoom-mcp.exe` was the Jun-14 pre-auth build. **It has since been rebuilt (2026-07-02 16:17, identical to `target-mcp/release/`)** — no repoint needed; only an MCP reconnect (`/mcp`) with Vuoom running under `VUOOM_ENABLE_CONTROL=1`. |

---

## 2. Fix plan (prioritized)

### Tier 1 — small verified bug fixes (hours)

1. **`key_chord` symbol keys** — in `key_to_vk` (`inject.rs`), map `"+"|"add"`→`VK_ADD` (0x6B — emits a literal `+` unshifted, unlike OEM_PLUS), `"*"|"multiply"|"asterisk"`→`VK_MULTIPLY` (0x6A), plus `subtract/divide/decimal/numpad0-9`. Unit test alongside `oem_punctuation_maps` (`inject.rs:651`). No control-server/MCP changes needed.
2. **`auto_speed` ergonomics** — surface `min_gap`/`lead`/`tail` as optional params on the MCP tool (defaults unchanged) AND state the 2.5 s threshold in the tool description so the agent doesn't trial-and-error it.
3. **Expose camera path in `clip_state`** — add sampled `(t, cx, cy, zoom)` from the already-computed `CameraTrack` to `ClipInfo`, and report the effective focus for Auto spans instead of `None`. This un-blinds the repair loop for wander/framing critique.
4. **Sidecar hygiene** — delete the divergent `target-mcp/` dir; document "rebuild → `/mcp` reconnect" in the MCP docs.

### Tier 2 — camera/direction features (the perceived-quality wins)

5. **Rect focus** — new `ZoomMode::Rect { rect }`; camera derives zoom = fit-rect-with-padding and focus = rect center. Extend `set_zoom_focus`/`add_zoom` to accept an optional rect. This is the "zoom to the result, not the cursor" enabler — the agent screenshots, finds the result region, and frames *that*.
6. **Expose merge/behavior knobs** — add `merge_gap`, `merge_radius`, `min_rezoom_interval`, `dead_zone` to `set_zoom_style` so "don't merge my 4 clicks" / "one deliberate zoom" is an API call, not a recompile.
7. **Per-span envelope** — optional `hl_zoom_in`/`hl_zoom_out` (and/or explicit hold) on `ZoomKeyframe`, settable via `update_zoom`, so a reveal span can ease slower and hold longer than setup spans.
8. **Caret-follow** — emit a position on `KeyType` events (caret/last-injection point) so `Auto` spans pan with typing; fixes "text runs off the right edge" without new camera machinery.

### Tier 3 — bigger product gaps

9. **Motion preview over the control protocol** — `PreviewClip { start, end, fps, width }`: reuse the `sample_frames` compositing loop over a range, encode a small low-res GIF with the existing encoder, return base64. Closes the "critique loop can't see motion" gap without full exports.
10. **Window-targeted capture** — two steps: (a) cheap: window-bounds helper (`DwmGetWindowAttribute(DWMWA_EXTENDED_FRAME_BOUNDS)`) + `set_region_to_window(title)` that snaps the existing `CropRegion` — kills the pixel-math, not the occlusion; (b) real: `windows_capture::window::Window` capture target parallel to `pick_monitor` — kills occlusion/overlay bleed entirely. This is the single biggest robustness win for AI-driven demos.

### Not code (director playbook — already in issues.md §3–4)

Storyboard-first, stage frame 1, zoom-to-reveal-once, hold the payoff, loop-friendly ends,
clean stage (no overlays/terminal). These should become the **prompting playbook** baked into the
vuoom MCP server instructions once Tier 2 gives the agent the tools to execute them.
