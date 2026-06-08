# 04 — Global Input Capture & the Auto-Zoom Algorithm

This is **the** document for Vuoom's signature feature. Part A captures global input with
frame-accurate timestamps. Part B turns that event log into cinematic, Screen-Studio-quality
camera motion. If any part of Vuoom must be excellent, it is this.

---

# PART A — Global Input Capture

## Decision

**Primary = Raw Input via `windows-rs`** (`RegisterRawInputDevices` with `RIDEV_INPUTSINK`),
running on a dedicated thread with a message-only window. Supplement absolute cursor position
with a high-rate `GetPhysicalCursorPos` poll. `rdev` is acceptable for a quick prototype only.

### Why Raw Input over low-level hooks

Microsoft's own guidance: *"In most cases where applications need to use low-level hooks, they
should monitor raw input instead, because raw input can asynchronously monitor mouse and keyboard
messages targeted for other threads more effectively than low-level hooks can."*

| | Low-level hooks (`WH_*_LL`) | **Raw Input** (`WM_INPUT`) | Polling |
|---|---|---|---|
| Global/background | Yes | Yes (needs `RIDEV_INPUTSINK` + valid `hwndTarget`) | Position only |
| On the input critical path | **Yes** (slow callback lags the whole system) | No (async) | No |
| Antivirus/EDR flagging | High (classic keylogger signature) | Lower | None |
| Recommended | Prototype only | **Production** | Supplement for absolute pos |

## Mechanics that must be correct

- **Dedicated thread + message-only window** (`HWND_MESSAGE`): create it, `RegisterRawInputDevices`
  for mouse+keyboard with `dwFlags = RIDEV_INPUTSINK` and `hwndTarget = <that window>`, then pump
  `GetMessage`/`PeekMessage` → `GetRawInputData(RID_INPUT)`. Without `RIDEV_INPUTSINK` you only
  get input while focused.
- **Never do work in the handler.** Stamp QPC, push the event to a lock-free queue, return. (Even
  for Raw Input keep the handler tiny.)
- **Raw mouse is deltas, not absolute position.** Use Raw Input for buttons/scroll/keys; sample
  `GetPhysicalCursorPos` on a ~240 Hz timer for a smooth absolute cursor path.

## Timestamps (the alignment guarantee)

- Cache `QueryPerformanceFrequency()` once. Stamp each event with `QueryPerformanceCounter()` the
  instant it arrives. Ignore `MSLLHOOKSTRUCT.time` (GetTickCount ms).
- WGC frames carry QPC-based `SystemRelativeTime` → input and frames live on one axis →
  frame-relative event offset is a subtraction. This is what makes auto-zoom land on the right frame.

## DPI & coordinates

- Per-Monitor-DPI-Aware-V2. Store all input in **physical virtual-desktop pixels** (which may be
  negative on secondary monitors). Use `GetPhysicalCursorPos`. Normalize to 0..1 against the
  captured monitor's physical rect only when feeding the planner. Mismatched DPI is the #1 source
  of "cursor and zoom don't line up" bugs.

## Known limitations to document for users

- **Admin/elevated windows block hooks (UIPI):** a non-elevated Vuoom won't see input over
  higher-integrity windows. To capture over admin apps, run Vuoom elevated (or signed
  uiAccess=true manifest). This is a Screen-Studio-class limitation — document it.
- **Code-sign the binary** to reduce AV friction from global input monitoring.

## Event log schema (drives auto-zoom)

```rust
enum InputEvent {
    Move   { qpc: i64, x: i32, y: i32 },           // physical virtual-desktop px
    Click  { qpc: i64, x: i32, y: i32, button: MouseButton },
    Scroll { qpc: i64, x: i32, y: i32, delta: i32 },
    DragStart { qpc: i64, x: i32, y: i32 },
    DragEnd   { qpc: i64, x: i32, y: i32 },
    KeyType   { qpc: i64 },                          // for "sustained typing" zoom hold
}
```

---

# PART B — The Auto-Zoom Algorithm

Goal: a "click around a UI" recording auto-produces zooms a non-expert calls *"professional /
like Screen Studio."* The approach is a hybrid of **Cap's** (read from source) and **Screen
Studio's** (reconstructed from teardowns), with conservative defaults.

## Mental model

1. **Plan** discrete `ZoomKeyframe`s from the click/activity log (click-driven, debounced).
2. **Animate** a virtual camera (center + zoom) toward those targets each output frame using
   **critically-damped springs** — self-correcting, no overshoot, frame-rate independent.
3. **Clamp** the camera so the cursor stays in frame and the viewport never reveals off-screen
   empty area.
4. Every auto keyframe is **editable** on the timeline (same data structure the editor mutates).

## Data model (Cap-style `ZoomSegment`)

```rust
struct ZoomKeyframe {
    start: f64,            // seconds, frame-relative (from QPC)
    end:   f64,
    amount: f64,           // zoom multiplier; 1.0 = no zoom
    mode: ZoomMode,        // Auto | Manual { x: f32, y: f32 }  (normalized 0..1)
    edge_snap_ratio: f64,  // pull focus toward edges so corner content isn't cut (default 0.25)
}
enum ZoomMode { Auto, Manual { x: f32, y: f32 } }
```

## The planner (log → keyframes)

```
for each click in event_log:
    seed a candidate zoom-in centered on the (smoothed) click position
merge candidates that are close in TIME (< MERGE_GAP) AND SPACE (< merge_radius)
    → collapses double/rapid clicks into one segment (no pulse-zoom)
extend a segment while a drag or continuous scroll or sustained typing is active
start each zoom PRE_ROLL before the click (anticipation)
zoom out only after HOLD seconds of no nearby activity
enforce a minimum re-zoom interval (frequency limit) to avoid motion sickness
```

## The camera (per-frame animation)

Two critically-damped springs — one 2D (`center`, normalized) and one scalar (`zoom`). Use the
**half-life-parameterized exact update** (frame-rate independent, no overshoot; from Orange
Duck's "Spring-Roll-Call"). Half-life = time to close half the remaining distance — intuitive to
tune.

```rust
const LN2: f64 = 0.69314718056;

/// Critically-damped spring, exact integration. `hl` = half-life (s).
fn spring_update(x: &mut f64, v: &mut f64, x_goal: f64, hl: f64, dt: f64) {
    let y = (4.0 * LN2) / hl / 2.0;          // damping derived from half-life
    let j0 = *x - x_goal;
    let j1 = *v + j0 * y;
    let eydt = (-y * dt).exp();
    *x = eydt * (j0 + j1 * dt) + x_goal;
    *v = eydt * (*v - j1 * y * dt);
}
```

Per output frame:

```rust
let target_center = clamp_camera(snap_to_edges(smoothed_cursor, edge_snap_ratio), target_amount);
spring_update(&mut center.x, &mut vel.x, target_center.x, HL_PAN,  dt);
spring_update(&mut center.y, &mut vel.y, target_center.y, HL_PAN,  dt);
spring_update(&mut zoom,     &mut zoom_v, target_amount,  HL_ZOOM, dt);
let center = clamp_camera(center, zoom);     // re-clamp after integration
```

### The off-screen guard (must-have)

Clamp the camera center so the zoomed viewport never extends past the captured frame (Cap's exact
formula):

```rust
fn clamp_camera(center: Vec2, zoom: f64) -> Vec2 {
    let half = 0.5 / zoom;                   // half viewport extent in normalized space
    Vec2 {
        x: center.x.clamp(half, 1.0 - half),
        y: center.y.clamp(half, 1.0 - half),
    }
}
```

`snap_to_edges` nudges the focus toward a screen edge when the cursor is near it (governed by
`edge_snap_ratio`) so corner UI isn't cropped, while keeping ~12–15% margin around the cursor.

### Jitter rejection (the "shaky → glide" pass)

- **Pre-smooth** the raw cursor before it becomes the pan target (a short spring or a 1€ filter).
  This is Screen Studio's hallmark "turn shaky movement into a smooth glide."
- **Dead-zone:** only move the pan target when the cursor leaves a box (~8–12% of frame) around
  the current camera center → micro-movements don't drag the camera.

## Cap's actual spring constants (reference)

Cap's `crates/rendering/src/zoom.rs` uses an explicit spring (not a plain cubic):

```
ZOOM_DURATION           = 1.0    // s
SCREEN_SPRING_STIFFNESS = 200.0
SCREEN_SPRING_DAMPING   = 40.0
SCREEN_SPRING_MASS      = 2.25
// → omega0 = sqrt(200/2.25) ≈ 9.43 rad/s ; zeta ≈ 0.943 (barely under-damped, no visible bounce)
// zoom-OUT uses omega ×0.9, zeta ×1.15 → faster, fully non-oscillating
```

If you prefer the stiffness/damping/mass form over half-life, use these as a starting point. Both
forms are equivalent; half-life is just easier to tune. **Verify against Cap's current `main`
before hard-coding — they iterate fast.**

## Default parameter set (Screen-Studio-quality starting point)

| Param | Default | Rationale |
|---|---|---|
| Zoom amount (click) | **1.8×** (range 1.5–2.0) | Cap/Screen-Studio typical |
| `HL_ZOOM` (zoom spring half-life) | **0.30 s** | snappy but smooth zoom-in |
| `HL_PAN` (pan spring half-life) | **0.22 s** | tracks cursor without rubber-banding |
| Zoom transition feel | ζ≈0.94 in, ζ≈1.0+ out | matches Cap |
| `HOLD` before zoom-out | **1.8 s** | Screen-Studio-like dwell |
| `PRE_ROLL` before click | **0.3 s** | anticipation |
| `MERGE_GAP` (debounce) | **0.8 s** | collapse rapid clicks |
| Spatial merge radius | **~15% of screen** | merge nearby clicks |
| Dead-zone | **~10% of frame** | jitter rejection |
| `edge_snap_ratio` | **0.25** | keep edge content in frame |
| Min re-zoom interval | **1.0 s** | frequency limit (no motion sickness) |

All of these are exposed in the editor with these as defaults (spec §5.1).

## Manual editing (layered on top of auto)

- Every auto keyframe is a draggable timeline block: add / remove / move / resize / re-target.
- Manual focus point is pixel-precise (`ZoomMode::Manual { x, y }`).
- The editor mutates the **same** `ZoomKeyframe` list the planner produced — no separate path.

## Acceptance criteria (from spec §5.1 — the M2 gate)

1. Camera renders smoothly at 60fps, no stutter/tearing.
2. Default settings on a typical UI recording → "professional / like Screen Studio."
3. No zoom ever shows empty space outside the captured content.
4. Auto-placed zooms are fully editable after recording.
5. Tiny cursor moves / accidental micro-clicks don't cause camera jumps.

## Implementation notes

- `vuoom-zoom` has **no GPU/OS dependencies** → unit-test the planner and springs against
  synthetic event logs (assert no off-screen reveal, no >N zooms/sec, debounce works).
- The compositor consumes the camera's `(center, zoom)` per frame as a transform uniform — see
  [`05-Compositing-and-Preview.md`](./05-Compositing-and-Preview.md).

## Confidence caveats

- Cap is open but iterates fast; its `zoom.rs` constants/clamp were read via fetch — re-verify.
- Screen Studio is closed/macOS; its specifics are reconstructed from reviews — directional, not exact.

## Sources

- Raw Input: <https://learn.microsoft.com/en-us/windows/win32/inputdev/using-raw-input> ·
  `RegisterRawInputDevices`: <https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-registerrawinputdevices> ·
  `LowLevelMouseProc`: <https://learn.microsoft.com/en-us/windows/win32/winmsg/lowlevelmouseproc>
- High-DPI: <https://learn.microsoft.com/en-us/windows/win32/hidpi/high-dpi-desktop-application-development-on-windows>
- QPC: <https://learn.microsoft.com/en-us/windows/win32/sysinfo/acquiring-high-resolution-time-stamps>
- Cap zoom source: <https://github.com/CapSoftware/Cap/blob/main/crates/rendering/src/zoom.rs> ·
  <https://github.com/CapSoftware/Cap/blob/main/crates/project/src/configuration.rs>
- Spring math: <https://theorangeduck.com/page/spring-roll-call> ·
  <https://www.ryanjuckett.com/damped-springs/>
- Screen Studio behavior (teardown): <https://scribehow.com/page/Screen_Studio_Review_2026_I_Tested_the_Auto-Zoom_Mac_Recorder_for_90_Days__Heres_the_Truth__0R7wu5TiSvqYAK3TzdygdQ>
- `rdev`: <https://docs.rs/rdev/>
