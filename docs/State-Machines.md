# State machines

Where in Vuoom something moves through steps, where it lives, and the rules that keep it
straight. Read this before touching one of these flows. If you add a flow with more than two
states, model it as one discriminated union (a `step` or `phase` field), not as several
booleans or nullable signals that have to be kept in agreement by hand, and add it here.

**The rule:** one value says which step a flow is in. Data that only exists in one step
(the backdrop of an overlay, the release being installed) lives *inside* that step's
variant, so it can't outlive it. An async continuation checks that the flow is still in the
step that started it before it writes anything.

**How this list was made**, so it can be redone: every boolean or nullable signal and every
`let` flag in `src/` (excluding `src/mock`), and every `AtomicBool`, `Mutex<Option<…>>` and
state-like enum in `crates/` and `src-tauri/`, was looked at. A rerun:

```sh
grep -rnE "createSignal(<[^>]*>)?\((false|true|null)\)|^\s*let \w+ = (false|true|null)\b" src --include=*.ts --include=*.tsx | grep -v src/mock
grep -rnE "AtomicBool|Mutex<Option<|enum \w*(State|Phase|Status|Mode|Step)\b" crates src-tauri/src --include=*.rs
```

Each hit is either a flow below, a plain setting or toggle (a pref, a panel open, hover), or
data (a selection, a cache). Only the flows are listed.

## Front end

### Record flow: `src/editor/recordFlow.ts`, driven by `createEditor.ts`

From a Record press to the overlay that owns the screen. `recordFlow()` holds one of:

| Step | On screen | Leaves to |
|---|---|---|
| `idle` | the editor | `pending` (`startRecord`) |
| `pending` | nothing new: a confirm dialog or engine call is in flight | `idle`, `choosing`, `pointing`, `framing` |
| `choosing` | the source picker (`SourcePicker.tsx`) | `framing` (`beginRecordWith`), `idle` (`closeSourcePicker`) |
| `pointing` | the point-at-a-window overlay (`WindowPicker.tsx`) | `pending` → `framing` / `choosing` / `idle`; straight to `idle` if the overlay can't come up |
| `framing` | the record overlay (`RecordOverlay.tsx`) | `idle` (`onRecordFinished` / `onRecordCancel` / `onRecordFailed`) |

- Record presses outside `idle` are ignored, so a double click or a held Ctrl+Shift+R starts
  one flow. A second click on a source or a window is ignored the same way.
- `pointing` and `framing` carry their `target` and `backdrop`. The backdrop is written only
  if the flow is still the exact step that asked for it (identity check), so a screenshot
  that arrives after a cancel, or after the user picked a window, is dropped.
- `ownsScreen(flow)` is true while the editor window is the fullscreen overlay; autosave
  stands down then.
- The current step is mirrored on the root element as `data-record-step` (for tests and
  agents driving the UI).

### Record overlay: `src/RecordOverlay.tsx`

`phase`: `select → preparing → countdown → recording → finalizing`. Diagram at the
component. A failed engine setup returns `preparing` to `select`; a capture that fails to
start ends the overlay (`onFailed`), and so does a failed stop: the engine takes the take
out before anything can fail and restores the editor window either way, so there is no live
take to go back to. If the failed take had replaced the loaded clip, the editor goes back to
Home instead of showing a clip the engine no longer has.
`paused` is a sub-state of `recording` (the elapsed readout freezes).

`control` tracks the overlay's hand-off to the editor: `open → closed` on Cancel / Esc,
`open → committed` when capture starts (the countdown reaches 0, or at once with no
countdown; Stop ends the take), `committed → closed` if capture fails to start or stop. Every
`await` in the start chain checks `control` before it goes on, so an Esc during
"Preparing…" can no longer shrink the window to the bar, leave the region border up, or
start a capture nobody can see. Cancel is disabled once `control` leaves `open`.

### Export dialog: `src/ExportDialog.tsx`

`phase`: `configure → starting → exporting → done`, with `error` (Try again / Back to
settings). Diagram at the `Phase` type. `fitting` is a separate flag while "Fit to size"
searches, and only exists inside `configure`.

### Auto-update: `createEditor.ts`

`updateFlow()`: `idle ⇄ checking → ready ⇄ installing → (restart)`. A failed install goes
back to `ready` so it can be retried. `update()`, `updating()` and `checkingUpdate()` are
views of it for the title bar and Settings. `updateNote()` is separate: the answer to the
last check the user asked for.

### Smaller ones

| Flow | Where | Shape |
|---|---|---|
| Captions job | `editor/captions.ts` | `job()`: `null` or `{ step: "start" \| "download" \| "listen" }` |
| Voice cleanup | `editor/audio.ts` | `cleaning()`: per-track busy map |
| Canvas gesture | `types.ts` `Drag` | `null` or one `mode` variant per gesture |
| Timeline gestures | `createEditor.ts` | `zoomDrag()` / `speedDrag()` / `cutDrag()` / `focusDrag()`: `null` or the drag in progress; `tlDrag` while scrubbing |
| Crop editing | `createEditor.ts` | `cropEdit()`: `null` or the crop to restore on cancel |
| Text editing | `createEditor.ts` | `editingText()`: `null` or the text's id |
| Settings | `createEditor.ts` | `settingsTab()`: `null` (closed) or the open tab |
| Camera preview | `components/CameraControls.tsx` | effect keyed by a generation counter; stale polls are dropped |
| Hotkey capture | `components/Settings.tsx` `HotkeyField` | `listening()` + the last `problem()` |
| Onboarding | `createEditor.ts` | `showWelcome` → (skip) → `coachRecord` |
| Busy veil | `createEditor.ts` | `busy()`: `null` or what the app waits on |
| Saving | `createEditor.ts` | `saving()`: a second Save while one runs is ignored |
| Unsaved edits | `createEditor.ts` | `dirty()` plus edit / persisted / exported version counters: what autosave, the dot by the name and "Discard edits?" go by |
| Playback | `createEditor.ts` | `playing()` drives one `requestAnimationFrame` loop |

### Latest-wins pumps

`pushSeek` (`seekBusy` + `seekPending`), `pushEdit` (`editsBusy` + `editQueues`) and
`createSyncSlot` in `src/sync.ts` share one two-state shape: idle, or running with the
newest request queued behind it. One engine call is in flight at a time, the last value is
never dropped, and an older failure can't undo a newer edit. Use `createSyncSlot` for a new
one rather than another pair of flags.

### Selection

`selected` (+ `selExtra` for a multi-selection), `selZoom`, `selSpeed`, `selCut` and
`selCaption`: **at most one kind is selected at a time.** Go through `selectAnnotation`,
`selectSegment` or `selectCaption`, or clear the other kinds in the same step, as the
pointer-up handlers do. An effect clears the caption whenever anything else is selected.
These are kept as separate signals on purpose: over 200 call sites read and write them,
and the rule above already holds everywhere, so folding them into one value would be churn
with no gain for the user.

### Dialog ownership

`isModal()` in `createEditor.ts` lists every dialog that owns the keyboard (export,
welcome, settings, palette, source picker). Editor shortcuts stand down while it is true.
A new dialog must be added there.

## Engine (Rust)

| Flow | Where | Shape |
|---|---|---|
| Engine boot | `src-tauri/src/lib.rs` `Engine` | `OnceLock<Result<Session, String>>`: booting → up or failed; the UI retries `preview_port` until up |
| Recording | `src-tauri/src/session.rs` `Session::active` | `Option<Active>`: no take, or one running; `Active::pauses` holds an open span while paused |
| Export / captions cancel | `Session::export_cancel`, `captions_cancel` | one flag each, reset when the next job starts (one runs at a time) |
| Loaded clip | `Session::edited` + `Session::current_recovery` | two mutexes that must name the same clip: none, loaded, or recording (edited emptied, recovery pointing at the new take). A take that fails to start puts the clip back; a failed added take restores the clip it was joining (`restore_base`); captions are only applied if `current_recovery` still names the clip they were made from |
| Next take's target | `Session::pending_window` / `pending_monitor` / `pending_region`, copied into `BorderState` | a display, a region on it, or a window. Set by `enter_overlay` (which fails if the window can't be recorded) and `set_region`, in that order |
| Add a take | `Session::append_next` | one-shot: armed by `set_append_take`, used up when a take starts (re-armed if the start fails) |
| Mic check | `Session::mic_check` | `Option<Recorder>` |
| Worker threads | capture, audio, camera, live preview, hotkey and zoom-chord watchers | each owns a `stop: AtomicBool`: running until it is set, then the thread winds down |

### Record window modes: `src-tauri/src/commands.rs`

The editor window is in one of three modes: **normal**, **overlay** (fullscreen, hidden
from capture: `enter_overlay`) and **stop bar** (`enter_stopbar`). Both `finish_recording`
and `cancel_record_flow` take it back to normal and drop what the take put up: the stop
hotkey watcher (`RecordingHotkey`), the region border (`BorderState`) and the panel's drag
limits (`drag_wall`). The front end's record flow decides when each runs. `enter_overlay`,
`set_region`, `enter_stopbar`, `set_panel_size`, `show_region_border` and
`cancel_record_flow` are synchronous, so Tauri runs them on the main thread in call order,
which `RecordOverlay`'s `control` relies on. `start_recording` and `finish_recording` are
async, which is why `control` has a `committed` step. The mode is not held in one Rust
value: it is the window's own properties plus those three slots, and changing that would
need testing on real displays. Teardown is safe to run any number of times.

Rules the engine does not enforce, so the front end must keep them:
- never `cancel_record_flow` while a take is running: it restores the window but does not
  stop the capture (the overlay's `control` turns Cancel off once capture starts);
- never `start_recording` twice: the second call is refused, but only after it has changed
  the window (the record flow allows one overlay at a time);
- any `finish_recording` error is final (see the record overlay above).

## Exercising them in a browser

`pnpm dev` and a plain browser run the whole UI against the mock engine (`src/mock`). Useful
URL flags:

| Flag | Effect |
|---|---|
| `?nowelcome` | no first-run card |
| `&onedisplay` | one display: Record goes straight to the record overlay |
| `&demo` | load the sample take; with it, `&export` opens the export dialog and `&record` starts the record flow |
| `&slow=set_region:900,start_recording:1200` | delay those engine commands (milliseconds), to reproduce races in the front end. The mock delays each command's effect, so it doesn't reproduce the real engine's in-order sync commands |
| `&update=1` / `&update=fail` | offer a mock release; its install succeeds / fails |

Read `document.querySelector(".app").dataset.recordStep` for the record flow's step, and
`window.__vuoomMockLog` for every engine command called, in order. For example, with
`&onedisplay&slow=set_region:900`, press Full screen, then Record, then Esc at once: the log
must show no `enter_stopbar`, `show_region_border` or `start_recording` after
`cancel_record_flow` (only the cleanup calls `set_append_take`, `set_camera_preview` and
`set_mic_check`).
