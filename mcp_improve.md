# MCP / AI Demo Director — Code Review & Improvement Plan

> **Implementation status (2026-07-02):** everything below through P2 is now implemented on
> this branch — the humanizer (§1.1–1.4), perception + repair loop (§2.1–2.6), injection
> correctness (§3.1–3.4), server hardening (§4.1–4.5), and the MCP-layer polish + tests
> (Parts 5–6). Still open: the §1.5 server-side `min_action_gap_ms` (the rhythm recipe went
> into the MCP instructions instead), §4.6 single-connection enforcement (documented,
> accepted), and the **v2 synthetic cursor rendering** (§1.1 endgame + roadmap item 19).
> All of it needs the runtime pass described in `docs/IMPLEMENTATION-STATUS.md`.

**Branch reviewed:** `feat/ai-demo-director-mcp` (13 commits over `main`)
**Files reviewed:** `crates/vuoom-mcp`, `crates/vuoom-control`, `crates/vuoom-input/src/inject.rs`,
`src-tauri/src/control_server.rs`, the `session.rs` additions, and the zoom pipeline
(`vuoom-zoom/planner.rs`, `camera.rs`, `config.rs`) these recordings flow through.

---

## TL;DR verdict

The architecture is genuinely good. The sidecar split (`vuoom-mcp` ↔ `vuoom-control` ↔ in-app
server), the newline-JSON protocol with round-trip tests, the "injected input flows through the
same hook as real input, so one click drives the app AND the zoom" trick — all of that is the
right design and it's clean, documented code. Nothing here needs a rewrite.

What's wrong is at a different level: **the tools produce robot-looking recordings and the
critique loop is half-blind.**

1. **The cursor teleports.** `click()` warps the mouse across the screen in a single
   `SendInput` batch. The hardware cursor is baked into the captured pixels, so the GIF shows
   a cursor that jumps instantly from A to B. No amount of camera-spring tuning can fix this —
   the smoothing happens *after* the damage is in the pixels. This is the single biggest reason
   agent recordings don't look "Screen-Studio smooth."
2. **Typing appears all at once, scrolling is one big jump, and there is no drag.** Same
   root cause: injection is instantaneous, real humans take time.
3. **The agent can't see while driving, doesn't know where the zooms are, and can't fix
   anything without a full re-record.** `clip_state` returns *counts*, not zoom times; there's
   no live screenshot; none of the session's existing edit ops (delete zoom, retime, add cut,
   auto-speed) are exposed.

Fix category 1+2 (a "humanizer" layer in `vuoom-input`) and category 3 (a handful of new
protocol ops that are one-line wrappers over code that already exists in `session.rs`), and
this feature goes from "somehow working" to actually delivering the demo-director story.

Everything below is ordered by impact, with file/line pointers and concrete fixes.

---

## Part 1 — Smoothness: why the recordings look robotic

### 1.1 Cursor teleportation is baked into the pixels (P0, the big one)

**Where:**
- `crates/vuoom-input/src/inject.rs:166` — `click()` sends `abs_move + down + up` in **one**
  `SendInput` batch. The cursor moves 0→100% of the distance in a single event.
- `crates/vuoom-capture/src/capture.rs:169` — `CursorCaptureSettings::Default` means Windows
  Graphics Capture draws the **hardware cursor into the frame pixels**.

**Consequence:** in the exported GIF the pointer visibly *teleports* between click targets.
The camera pre-smoothing spring (`camera.rs:161`, `hl_cursor = 0.12s`) only smooths the
*camera focus*, not the visible pointer — the pointer itself is already burned into the
captured frames. Two further knock-on effects:

- **Camera whip.** A cross-screen teleport is a step-function input to the pan spring. With
  `hl_pan = 0.22s` the camera races across the frame at high peak velocity; at 20 fps GIF
  output with no motion blur, that reads as a jerk, not a glide.
- **Zoom centring races the click.** The planner's `pre_roll = 0.3s` starts the zoom-in
  *before* the click (good), but the cursor only arrives at the click position at the exact
  click instant — so during the anticipation window the camera is zooming toward a point the
  pointer isn't at yet.

**Fix (recommended): humanized cursor motion in `vuoom-input`.**

Add an eased, interpolated move and make `click` glide by default:

```rust
/// Glide the cursor from its current position to (x, y) over `duration`,
/// injecting absolute moves at ~120 Hz along a minimum-jerk profile.
pub fn move_cursor_smooth(x: i32, y: i32, duration_ms: u32) {
    // s(t) = 10t^3 - 15t^4 + 6t^5  (minimum-jerk; starts and ends at zero velocity)
    // sample every ~8 ms; add a ±1px of low-amplitude noise if you want extra realism
}
```

- Duration should scale with distance (Fitts-like), e.g.
  `clamp(150 + px_distance / 3, 200, 900)` ms. Expose it as an optional `duration_ms`
  param on `move_cursor` / `click` so the agent can override pacing.
- `click()` becomes: glide → settle ~100–150 ms → down → 40–80 ms → up. The settle pause
  matters twice: it looks deliberate, *and* it gives the camera's pre-roll a stationary
  target to zoom toward.
- Use `GetCursorPos` for the start point so the path is continuous from wherever the
  pointer currently is.
- Bonus: the glide generates a stream of `WM_MOUSEMOVE` hook events, so the recorded cursor
  track (which the camera follows) becomes a real path instead of a step function — the pan
  spring output becomes silky *for free*.

**Fix (bigger, optional but the "pro" endgame): synthetic cursor rendering.**
Capture with `CursorCaptureSettings::WithoutCursor` and draw the cursor in the compositor
from the (smoothed) input track — the Screen Studio approach. That decouples the visible
pointer from OS jitter entirely, lets you scale the cursor when zoomed in, ease it
independently, and even smooth *human* recordings. This is a bigger lift (cursor sprite
rendering in `vuoom-render`) and can come after the humanizer, but it's where "fully
smooth and perfect" ultimately lives. Consider it the v2 of cursor flow.

### 1.2 Typing appears instantaneously (P0)

**Where:** `crates/vuoom-input/src/inject.rs:232` — `type_text` pushes one down/up pair per
UTF-16 unit into a **single** `SendInput` batch. A whole sentence materializes in ~0 ms.

**Consequences:**
- In the GIF, text pops into existence — instantly recognizable as fake.
- The zoom hold logic (`planner.rs`, activity extension) sees all key events at essentially
  the same timestamp, so a "typing" zoom doesn't sustain the way it would for a human; the
  camera can zoom out mid-"typing".
- Some apps drop events when a huge batch arrives at once.

**Fix:** pace it. Add an optional `cps` (or `delay_ms`) parameter to `TypeText` (protocol +
tool), default ~12–18 chars/sec with ±30% jitter, injecting one char (or one key pair) per
tick. Trivial to implement server-side with `std::thread::sleep` on the connection thread.
Paced key events also sustain the zoom hold correctly because each keystroke is a fresh
activity timestamp.

### 1.3 Scroll is one violent jump (P1)

**Where:** `inject.rs:185` — `scroll()` sends the *entire* delta as a single wheel event
(`delta * WHEEL_DELTA`).

**Fix:** step it — one notch every 30–60 ms (optionally with an eased distribution: fast
in the middle, slow at the ends). Smooth-scrolling apps animate nicer with several small
deltas, and the recorded motion reads as intentional. Add optional `duration_ms`.

### 1.4 There is no drag (P1)

Sliders, drag-and-drop, text selection, window moves — common demo actions — are impossible
with the current tool set. Add:

```
drag { x1, y1, x2, y2, button?, duration_ms? }
```

= glide to (x1,y1) → button down → eased move path to (x2,y2) (the same humanizer path
generator as 1.1) → button up. Drag-starts already count as zoom triggers in the planner
(`is_click_trigger`), so this integrates with auto-zoom for free.

### 1.5 Rhythm is entirely the agent's problem (P2)

The demo's pacing currently depends on the agent remembering to call `wait` with good
values. Two cheap improvements:

- Put a concrete **rhythm recipe** in the MCP `INSTRUCTIONS` string (`vuoom-mcp/src/main.rs:487`),
  e.g.: *"per step: move (auto-glide) → wait 300–500 ms → click → wait 800–1500 ms for the
  UI to react; keep total takes under ~30 s; end with wait 1000 ms so the final zoom-out
  completes before stop_recording."* The last point matters: if the agent stops recording
  immediately after the last click, the clip ends mid-zoom — always advise a tail wait
  ≥ `hold + zoom-out time` (~2.5 s).
- Optionally: a server-side `min_action_gap_ms` so back-to-back injections are never
  physically implausible even if the agent forgets to wait.

### 1.6 Zoom-config defaults are tuned for humans, not agents (P2)

`ZoomConfig::default()` (`vuoom-zoom/src/config.rs:44`) — `merge_gap 0.8`, `hold 1.8`,
`min_rezoom_interval 1.0` — were tuned for noisy human clicking. Agent clicks are sparse and
deliberate, so the defaults mostly work, but the agent has no way to adjust feel (e.g. longer
`hold` for a slow narration-style demo, tighter `pre_roll`). Consider one protocol op:

```
set_zoom_style { hold?, pre_roll?, hl_zoom?, hl_pan? }   // for the NEXT recording
```

Not urgent, but it's the knob the critique loop will eventually want ("the zoom leaves too
early → increase hold → re-record" is a much better iteration than blind re-recording).

---

## Part 2 — The critique loop is half-blind

The whole moat (per `docs/13-AI-Demo-Director-Research.md`) is *record → watch → critique →
improve*. Right now the "watch" and "improve" halves are severely under-equipped.

### 2.1 The agent cannot see the screen while driving (P0)

There is no live-screenshot tool. During the drive phase the agent clicks blind — it must
already know coordinates from some other source. The first mislocated click derails the
recording and the agent only finds out after `stop_recording` → `get_frames`.

**Fix is nearly free:** `Session::screenshot()` **already exists** (used as the region-picker
backdrop; it returns a base64 PNG data URL). Expose it:

- protocol: `Screenshot { width: Option<u32> }` → `ControlResponse::Frames`-style single PNG
- tool: `screenshot` — "See the current screen NOW (before/while recording) to locate what
  to click."

This is the highest-leverage 30 lines on this list. It turns "the agent needs a second
computer-use tool to find coordinates" into a self-contained loop: screenshot → locate →
click → screenshot → verify → next step.

### 2.2 The agent is told to critique zooms but is never told where they are (P0)

The `get_frames` tool description says *"sample sparsely, e.g. around each zoom"* — but
`stop_recording` returns only a zoom **count**, and `clip_state`
(`src-tauri/src/session.rs:836`) returns `{duration, zooms: usize, cuts, speed_regions}`.
The agent has no way to know *when* the zooms happen or *where* they centre.

**Fix:** return the segments. Extend `ClipInfo` (or the `RecordingSummary`) with:

```rust
pub struct ZoomSpan { pub start: f64, pub end: f64, pub amount: f64 }
// plus cuts: Vec<(f64, f64)>, and output_duration (see 2.4)
```

Then the agent can deterministically sample `[start - 0.2, midpoint, end + 0.2]` per zoom
instead of guessing timestamps across the whole clip. This also cuts vision-token cost —
fewer wasted frames.

### 2.3 The agent can't fix anything — its only tool is a full re-record (P0)

`session.rs` already implements the entire edit surface the Tauri UI uses: delete/retime
zoom keyframes, `add_cut`/`update_cut`/`delete_cut` (session.rs:1078+), `add_speed_region`,
`auto_speed` (session.rs:964), `clear_speed`, trim. **None of it is exposed over the control
protocol.** So when the critique finds "dead air from 4–7 s" or "the second zoom is
mis-centred", the only remedy is re-driving the whole target app — the most expensive and
least deterministic operation available.

**Fix:** expose the existing ops (each is a ~5-line dispatch arm + protocol variant):

| New op | Wraps | Critique it fixes |
|---|---|---|
| `list_zooms` / extended `clip_state` | project.zooms | (see 2.2) |
| `remove_zoom { index }` | existing zoom CRUD | "zoom on a misclick / wrong target" |
| `add_zoom { start, end, x, y, amount }` | existing zoom CRUD (Manual mode) | "this moment needed a zoom" |
| `update_zoom { index, ... }` | existing zoom CRUD | "zoom leaves too early / wrong centre" |
| `add_cut { start, end }` | `Session::add_cut` | "dead air / loading spinner" |
| `auto_speed { factor }` | `Session::auto_speed` | "the idle stretches drag" |
| `set_trim { start, end }` | existing trim | "trim the awkward first second" |

This changes the iteration economics completely: re-record only when the *content* is wrong;
repair timing/framing in place. It's also exactly the "bounded, convergent verify loop" the
research doc calls for — edits are deterministic, re-records are not.

### 2.4 `get_frames`/`seek` sample source time, but export runs on the output timeline (P1 correctness bug)

- Export maps output→source through cuts + speed: `session.rs:565–585`
  (`out_mapping` / `output_to_source`).
- `sample_frames` (`session.rs:851,868`) and `seek` (`session.rs:476,482`) index frames by
  **raw source time** with no remap, and `clip_info.duration` is the **source** duration.

So as soon as the agent uses `set_paused` (paused spans become cuts at stop:
`session.rs:431–447`) or any speed region exists, the timestamps the agent critiques do
**not** correspond to the exported GIF's timeline — frames inside cuts are even sampleable
though they'll never appear in the export. The critique loop is then judging footage that
doesn't ship.

**Fix:** make the control-facing sampling operate in **output time**: map each requested `t`
through `output_to_source` (the code already exists for export), and report
`output_duration` in `clip_state` alongside the source duration. That way `get_frames(t)`
shows *exactly* what the exported GIF shows at `t`. (The editor UI can keep source-time
seeking; this only needs to change in the control path or behind a flag.)

### 2.5 No status / no cancel / no recovery (P1)

If a tool call fails mid-flow (or the MCP client restarts), the agent has no way to ask
"am I recording right now?" and no way to abandon a botched take other than `stop_recording`
(which builds the full editable clip just to throw it away). Add:

- `status` → `{ state: idle|recording|paused|clip_ready, elapsed_s, region, zoom_amount }`
- `cancel_recording` → stop + discard without building the project.

Cheap, and it makes the sidecar robust to the messy reality of agent sessions.

### 2.6 `get_frames` has no default downscale (P2)

`GetFrames.width` is optional and unset means full output resolution — a 2560-px-wide
base64 PNG per sampled frame straight into the agent's context. Default to ~800 px in the
MCP tool layer (agent can still ask for more) to protect context/token budgets. Also
consider capping `times.len()` (e.g. 16) with a clear error, so a confused agent can't
request 200 frames.

---

## Part 3 — Input injection correctness (works-on-my-machine bugs)

### 3.1 `SendInput` failures are silently swallowed (P1)

`inject.rs:128–131`: `let _sent = unsafe { SendInput(...) }`. If the target window is
**elevated** (UIPI blocks injection from a non-elevated Vuoom), or input is otherwise
rejected, every click/keystroke silently no-ops. The agent then records a demo of nothing
happening and can't tell why.

**Fix:** return the injected count, compare to `inputs.len()`, and propagate an error
(`"input injection blocked — is the target app elevated?"`) through
`ControlResponse::Error` so the agent sees it immediately. This also means the injection
functions should return `Result`, and `control_server.rs` should map them with `unit()`
like the stateful ops.

### 3.2 Extended-key flag is missing (P1)

`inject.rs:211` (`vk_event`) never sets `KEYEVENTF_EXTENDEDKEY`. Arrows, Home/End,
PgUp/PgDn, Insert/Delete are *extended* keys; without the flag some apps (terminals,
anything reading scan codes, RDP sessions, some games/Electron apps) will interpret them as
numpad keys — e.g. "down arrow" becomes "numpad 2". Set the flag for the extended VK set.
While in there, consider also sending real scan codes (`MapVirtualKeyW`) alongside VKs —
some apps ignore VK-only injection.

### 3.3 `key_to_vk` has no punctuation keys (P1)

`inject.rs:51` covers modifiers, named keys, letters, digits, F1–F12 — but **no OEM keys**.
Chords like `ctrl+=` / `ctrl+-` (zoom in every browser — practically the first thing a demo
agent will try), `ctrl+/` (comment toggle), `ctrl+.` etc. return `unknown key`. Add the
common `VK_OEM_*` mappings: `- = [ ] ; ' , . / \` `` ` ``.

### 3.4 `type_text` and control characters (P2)

`\n` typed as a Unicode code unit (0x0A) does not reliably activate default buttons or
submit forms the way a real Enter press does; `\t` similarly won't move focus in many
controls. Map `\n` → VK_RETURN press and `\t` → VK_TAB press inside `type_text`, unicode
for everything else.

### 3.5 Double-click nuance (P3, fine as-is)

The down/up/down/up single batch registers as a double-click (same position, ~0 ms apart —
within `GetDoubleClickTime`), so this works; once pacing (1.2) exists, keep the pair gap
under ~100 ms deliberately. No action needed beyond not breaking it.

---

## Part 4 — Protocol & server robustness

### 4.1 No authentication on the control server (P1, security)

`control_server.rs:31` binds loopback and gates on the env var — good — but once enabled,
**any local process** can read `%TEMP%\vuoom-control.json` and inject arbitrary input
(keystrokes into whatever is focused!). Loopback is not a trust boundary on a multi-process
machine, and this ships in a public repo advertising the port file location.

**Fix (small):** generate a random token at startup, write it into the port file
(`{"port": N, "token": "..."}`), require it — simplest form: first line of each connection
must be the token or the server drops the socket. `discover_port` grows into
`discover_endpoint() -> (port, token)`. ~40 lines total across the two crates.

Related nits:
- `VUOOM_ENABLE_CONTROL=0` currently **enables** the server (`control_server.rs:27` checks
  presence, not value). Treat `0`/`false`/empty as off.
- Delete the port file on graceful shutdown so a stale file doesn't point a future sidecar
  at a dead (or worse, someone else's) port. Cheap partial: also write the PID and let the
  sidecar sanity-check it.

### 4.2 No timeouts anywhere on the wire (P1)

`vuoom-control`'s `Client::call` (`lib.rs:337`) does a blocking `read_line` with **no read
timeout**, and `connect` with no connect timeout. If Vuoom hangs (or a long export is in
progress on the server side of the same pipeline), the MCP tool call hangs forever, which
then wedges the agent's turn. Set `set_read_timeout` / `connect_timeout` — generous for
export (minutes), short for everything else. Which leads to…

### 4.3 Long exports block the whole bridge (P2)

Two compounding designs:
- `Bridge` (`vuoom-mcp/src/main.rs:38–60`) holds the connection `Mutex` across the entire
  blocking call — so during a 90-second GIF export, even `ping` blocks.
- `export_gif` in `dispatch` runs synchronously on the connection thread with a no-op
  progress callback (`control_server.rs:185`).

**Fix:** make export a job: `ExportGif` returns immediately with a job id;
`export_status { id }` → `{ done, total, finished, error?, path }`. The session already
threads a `progress(done, total)` callback through export — plumb it into a shared job
table. The agent gets progress narration for free, and per-call timeouts (4.2) can stay
short. (Alternative minimal fix: keep sync export but open a *dedicated* connection per
tool call so `ping`/`status` never queue behind it.)

### 4.4 `pending_auto_click` leaks into interactive recordings (P1, sneaky)

`start_recording` (tool) sets `SetAutoZoomOnClick { on: true }` and the flag is **sticky**
(`session.rs`, `pending_auto_click` applies to "the next recording"). Sequence: agent
records a demo → human later hits record in the UI → their recording unexpectedly zooms on
every click, because the agent's flag was never reset. Also, the tool's two-call sequence
(`main.rs:290–295`) isn't atomic — if `StartRecording` fails, the flag is still flipped.

**Fix:** carry the flag *in* `StartRecording { auto_zoom_on_click: Option<bool> }` instead
of a separate sticky setter, or reset the pending flag to the config default inside
`stop_recording`. One round-trip fewer, one race fewer, zero cross-contamination.

### 4.5 `wait` is unclamped (P2)

`main.rs:384` sleeps for any `ms` the model passes. A hallucinated `ms: 3600000` wedges the
turn. Clamp to something sane (e.g. 15 000 ms) and say so in the description ("for longer
waits, call wait repeatedly") — this also keeps each tool call within client timeouts.

### 4.6 Concurrent clients can interleave stateful ops (P3)

Each accepted connection gets its own thread (`control_server.rs:47`) and stateful ops have
no cross-connection coordination — two sidecars could interleave `start/stop`. Given the
opt-in, single-user posture this is acceptable; if you add the token (4.1), consider also
accepting only one active control connection at a time. Document it either way.

---

## Part 5 — MCP-layer polish

- **Tool annotations.** rmcp supports MCP tool annotations — mark `ping`, `screen_geometry`,
  `clip_state`, `get_frames`, `estimate_gif` as `readOnlyHint`, and `export_*` /
  input-injection tools as non-idempotent/destructive. Clients use these for permissions UX;
  it's metadata you get almost for free.
- **Structured errors.** Everything maps to `McpError::internal_error` (`main.rs:88–90`).
  Distinguish at least "Vuoom not running / control disabled" (actionable: tell the user to
  launch with `VUOOM_ENABLE_CONTROL=1`) from "bad request" from "op failed" — the agent
  reacts very differently to each.
- **`unexpected: {other:?}` responses** (e.g. `main.rs:250`) return success with a debug
  string. Make protocol mismatches an *error* so the agent doesn't parse Rust debug output.
- **Instructions string** (`main.rs:487`): after Part 1 lands, encode the winning recipe —
  glide-then-click defaults, the rhythm guidance, the "tail wait ≥ 2.5 s before
  stop_recording" rule, "sample frames only around zoom spans," "prefer edit ops over
  re-recording," and the 3–4 iteration cap from the research doc. The instructions are the
  cheapest place to raise output quality across every client.
- **rmcp pin:** `rmcp = "1.7"` — pinned as the research doc recommended. Good; revisit at 2.0.

---

## Part 6 — Testing gaps

Current tests (protocol round-trips, `normalize_abs`, `key_to_vk`, mock control server
example) are solid for the pure layers. Missing:

1. **Sidecar ↔ mock-server integration test in CI** — the `mock_server.rs` example exists
   but nothing exercises `vuoom-mcp` against it automatically (tools/list is smoke-tested
   locally per the docs; make it a real `#[test]` or CI step, including one full
   tool-call round trip and the error path when the server is absent).
2. **Humanizer unit tests** (once built): the path generator is pure math — assert monotone
   progress, zero end velocity, duration scaling, and that the emitted event count matches
   the sample rate.
3. **Output-time mapping test** for 2.4: with one cut, assert `sample_frames(t)` equals the
   frame `export` emits at `t`.
4. **A scripted "golden demo"** (drive Notepad or the Vuoom window itself: click, type,
   scroll, export) runnable on a real machine as the manual runtime checklist —
   `docs/AI_DEMO_DIRECTOR.md` already lists what needs runtime verification; turn it into a
   copy-pasteable script.

---

## Prioritized roadmap

**P0 — makes recordings smooth and the loop functional (do these first):**
1. Humanized cursor glide (`move_cursor_smooth`, glide-then-click default, distance-scaled
   duration) — §1.1
2. Paced typing (`cps` param) — §1.2
3. `screenshot` tool (live perception; `Session::screenshot` already exists) — §2.1
4. Zoom spans + cuts + output duration in `clip_state`/`stop_recording` — §2.2
5. Edit ops over the protocol (remove/add/update zoom, add_cut, auto_speed, trim) — §2.3

**P1 — correctness & robustness:**
6. Stepped scroll + `drag` tool — §1.3, §1.4
7. `SendInput` failure detection (UIPI) — §3.1
8. `KEYEVENTF_EXTENDEDKEY` + OEM punctuation keys — §3.2, §3.3
9. Output-time frame sampling — §2.4
10. `status` + `cancel_recording` — §2.5
11. Auth token in port file; honor `VUOOM_ENABLE_CONTROL=0`; stale-file cleanup — §4.1
12. Client read/connect timeouts — §4.2
13. Fix `pending_auto_click` stickiness (flag inside `StartRecording`) — §4.4

**P2 — polish:**
14. Async export job + progress — §4.3
15. `get_frames` default width + count cap — §2.6
16. `wait` clamp — §4.5
17. `set_zoom_style` op; rhythm recipe in INSTRUCTIONS; tool annotations; structured
    errors — §1.5, §1.6, Part 5
18. Sidecar↔mock CI integration test + golden demo script — Part 6

**v2 — the endgame for "perfect":**
19. Synthetic cursor rendering (capture without cursor, draw smoothed cursor in the
    compositor, scale it under zoom) — §1.1. This is the Screen-Studio-grade cursor and it
    improves *human* recordings too.

---

## What you did right (keep these)

- The sidecar/protocol/in-app split with a dependency-light contract crate and round-trip
  tests — textbook.
- Injected input flowing through the real low-level hook so one mechanism drives the app
  *and* the zoom planner — the elegant core insight, verified in code.
- Opt-in env gate + loopback bind for an input-injection surface (just add the token).
- Deterministic camera track shared by scrub/export; critically-damped springs with exact
  integration (no overshoot) — the zoom *math* is not your problem; the *inputs* to it are.
- Honest docs: the research file's risk table ("desktop driving 8/10, sync 8/10") is exactly
  where the runtime issues live. The docs predicted the pain points; Part 1–2 above is how
  to pay them down.
