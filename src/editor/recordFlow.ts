// The record flow as one explicit state machine: from a Record press to the overlay that
// frames, counts down and records the take. One value says which step is on screen, so the
// steps can never overlap (a source picker under the overlay, a second overlay from a
// double click), and whatever ends a flow, it ends on `idle`.
//
//   idle ──startRecord──▶ pending ──┬──▶ choosing ──pick a source──▶ framing
//     ▲                             ├──▶ pointing ──pick a window──▶ pending ──▶ framing
//     │                             └──▶ framing     └─"list"────▶ pending ──▶ choosing
//     └──── declined / closed / cancelled / finished / failed (from any step) ────┘
//
// The overlay steps (`pointing`, `framing`) go straight back to `idle` if the engine can't
// bring the overlay up. The backend's "add this take to the clip" switch is set on every
// startRecord, so leaving early never has to reset it.
//
// See docs/State-Machines.md for the other state machines in the app.
import type { PickableWindow } from "../components/WindowPicker";

/** What a take records: one display (a region on it can be framed) or one app window. */
export type RecordTarget =
  | { kind: "display"; name: string; label: string }
  | { kind: "window"; hwnd: number; label: string };

/** Pointing at a window to record: the windows on the covered display, front to back, and
 *  that display's origin on the virtual desktop. */
export interface WindowPick {
  windows: PickableWindow[];
  origin: { x: number; y: number };
}

export type RecordFlow =
  /** Nothing under way: the editor owns the screen. */
  | { step: "idle" }
  /** Between steps: a confirm dialog or an engine call is in flight. Record presses are
   *  ignored, so a double click or a held shortcut starts one flow, not two. */
  | { step: "pending" }
  /** The source picker dialog: which display or window to record. */
  | { step: "choosing" }
  /** The point-at-a-window overlay over `target`'s display. `backdrop` is the frozen
   *  screenshot behind it, `null` until the engine sends it (or if the grab failed). */
  | { step: "pointing"; pick: WindowPick; target: RecordTarget; backdrop: string | null }
  /** The record overlay (RecordOverlay.tsx) owns the screen: region, countdown, the take
   *  and its save. It has its own phases and reports back through finished / cancel / failed. */
  | { step: "framing"; target: RecordTarget; backdrop: string | null };

export type RecordStep = RecordFlow["step"];

export const IDLE: RecordFlow = { step: "idle" };
export const PENDING: RecordFlow = { step: "pending" };

/** The editor window is the fullscreen overlay (it is hidden from the capture). */
export const ownsScreen = (f: RecordFlow): boolean => f.step === "pointing" || f.step === "framing";
