import { createSignal, Show, type JSX } from "solid-js";

/**
 * A numeric field you can **drag to scrub**, **click to type**, or **step with the
 * keyboard** — the core inspector ergonomic borrowed from pro editors. Horizontal drag
 * changes the value; hold Shift for a coarse (×8) sweep or Ctrl/Cmd for fine (×0.15)
 * control. When focused it behaves as an ARIA spinbutton: Arrow keys step by `step`
 * (Shift ×10), Page keys step by ×10, Home/End jump to the bounds, and Enter or typing a
 * digit drops into the text-edit mode. `onInput` fires live during the gesture (for
 * preview); `onCommit` fires at each committed change (the undo boundary) — a keyboard
 * step is treated as one discrete committed change. A `null` value renders an em-dash for
 * mixed/unknown selections.
 */
export interface ScrubFieldProps {
  value: number | null;
  min: number;
  max: number;
  /** Snap grid in stored units. Also sets the displayed precision. */
  step: number;
  /** Value change per pixel of drag. Default spreads the range over ~320px. */
  sensitivity?: number;
  /** Multiplies the stored value for display (e.g. 100 shows a 0–1 value as a percent). */
  displayScale?: number;
  suffix?: string;
  disabled?: boolean;
  title?: string;
  /** Continuous, during scrub/type — use for a live preview. */
  onInput?: (v: number) => void;
  /** Once, when the gesture ends (release / Enter / blur) — the undo boundary. */
  onCommit: (v: number) => void;
}

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));
const decimalsOf = (step: number) => {
  if (!Number.isFinite(step) || step <= 0) return 0;
  return clamp(Math.ceil(-Math.log10(step)), 0, 4);
};

export default function ScrubField(props: ScrubFieldProps): JSX.Element {
  const [editing, setEditing] = createSignal(false);
  const [draft, setDraft] = createSignal("");
  // Overrides props.value while a drag is in flight, so the readout tracks the cursor
  // even before the parent round-trips the committed value back through props.
  const [live, setLive] = createSignal<number | null>(null);
  // When true, the edit input seeds the caret at the end (typed-to-edit) instead of
  // selecting all (clicked/Enter-to-edit).
  const [seedAtEnd, setSeedAtEnd] = createSignal(false);
  let scrubEl: HTMLDivElement | undefined;

  const scale = () => props.displayScale ?? 1;
  const sens = () => props.sensitivity ?? (props.max - props.min) / 320;
  const snap = (v: number) => {
    const n = Math.round(v / props.step) * props.step;
    return Number(n.toFixed(decimalsOf(props.step) + 2));
  };
  const fmt = (storeVal: number) =>
    (storeVal * scale()).toFixed(decimalsOf(props.step * scale()));
  const shown = () => live() ?? props.value;

  let startX = 0;
  let startVal = 0;
  let moved = false;
  let activePointer = -1;

  const onPointerDown = (e: PointerEvent) => {
    if (props.disabled || editing()) return;
    e.preventDefault();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    activePointer = e.pointerId;
    startX = e.clientX;
    startVal = props.value ?? (props.min + props.max) / 2;
    moved = false;
  };
  const onPointerMove = (e: PointerEvent) => {
    if (activePointer !== e.pointerId) return;
    const dx = e.clientX - startX;
    if (!moved && Math.abs(dx) < 3) return;
    moved = true;
    const mod = e.shiftKey ? 8 : e.ctrlKey || e.metaKey ? 0.15 : 1;
    const v = clamp(snap(startVal + dx * sens() * mod), props.min, props.max);
    setLive(v);
    props.onInput?.(v);
  };
  const onPointerUp = (e: PointerEvent) => {
    if (activePointer !== e.pointerId) return;
    activePointer = -1;
    if (moved) {
      const v = live();
      if (v !== null) props.onCommit(v);
      setLive(null);
    } else {
      // A plain click (no drag) → switch to type mode with the whole value selected.
      beginEdit(props.value === null ? "" : fmt(props.value), false);
    }
  };

  // Enter text-edit mode. `atEnd` places the caret after the seed text (typed-to-edit);
  // otherwise the seed is selected so the first keystroke replaces it (clicked/Enter).
  const beginEdit = (seed: string, atEnd: boolean) => {
    setSeedAtEnd(atEnd);
    setDraft(seed);
    setEditing(true);
  };

  const commitEdit = (raw: string, refocus: boolean) => {
    setEditing(false);
    if (refocus) queueMicrotask(() => scrubEl?.focus());
    const cleaned = raw.replace(/[^0-9.+-]/g, "").trim();
    if (cleaned === "") return;
    const parsed = Number(cleaned);
    if (!Number.isFinite(parsed)) return;
    props.onCommit(clamp(snap(parsed / scale()), props.min, props.max));
  };

  // A single keyboard step is one discrete, committed change: preview via onInput then
  // draw the undo boundary via onCommit, mirroring how a mouse gesture settles.
  const stepTo = (target: number) => {
    const v = clamp(snap(target), props.min, props.max);
    props.onInput?.(v);
    props.onCommit(v);
  };
  const stepBy = (deltaSteps: number) => {
    const base = props.value ?? (props.min + props.max) / 2;
    stepTo(base + deltaSteps * props.step);
  };

  const onKeyDown = (e: KeyboardEvent) => {
    if (props.disabled) return;
    // Keys the field consumes must not leak to the app's global shortcuts (Z/X/C, space…).
    const handled = () => {
      e.preventDefault();
      e.stopPropagation();
    };
    switch (e.key) {
      case "ArrowUp":
        handled();
        stepBy(e.shiftKey ? 10 : 1);
        return;
      case "ArrowDown":
        handled();
        stepBy(e.shiftKey ? -10 : -1);
        return;
      case "PageUp":
        handled();
        stepBy(10);
        return;
      case "PageDown":
        handled();
        stepBy(-10);
        return;
      case "Home":
        handled();
        stepTo(props.min);
        return;
      case "End":
        handled();
        stepTo(props.max);
        return;
      case "Enter":
      case "F2":
        handled();
        beginEdit(props.value === null ? "" : fmt(props.value), false);
        return;
      default:
        // Typing a digit, sign or decimal point drops into edit mode seeded with it.
        if (e.key.length === 1 && /[0-9.+-]/.test(e.key) && !e.ctrlKey && !e.metaKey) {
          handled();
          beginEdit(e.key, true);
        }
    }
  };

  return (
    <Show
      when={!editing()}
      fallback={
        <input
          class="scrub-input"
          type="text"
          spellcheck={false}
          value={draft()}
          ref={(el) => queueMicrotask(() => {
            el.focus();
            if (seedAtEnd()) el.setSelectionRange(el.value.length, el.value.length);
            else el.select();
          })}
          onInput={(e) => setDraft(e.currentTarget.value)}
          onKeyDown={(e) => {
            e.stopPropagation();
            if (e.key === "Enter") {
              e.preventDefault();
              commitEdit(e.currentTarget.value, true);
            } else if (e.key === "Escape") {
              e.preventDefault();
              setEditing(false);
              queueMicrotask(() => scrubEl?.focus());
            }
          }}
          onBlur={(e) => commitEdit(e.currentTarget.value, false)}
        />
      }
    >
      <div
        ref={scrubEl}
        class="scrub"
        classList={{ disabled: !!props.disabled, mixed: shown() === null }}
        title={props.title}
        tabindex={props.disabled ? -1 : 0}
        role="spinbutton"
        aria-label={props.title}
        aria-disabled={props.disabled ? "true" : undefined}
        aria-valuemin={props.min * scale()}
        aria-valuemax={props.max * scale()}
        aria-valuenow={shown() === null ? undefined : shown()! * scale()}
        aria-valuetext={
          shown() === null ? "mixed" : `${fmt(shown()!)}${props.suffix ?? ""}`
        }
        onKeyDown={onKeyDown}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
      >
        <span class="scrub-val">
          <Show when={shown() !== null} fallback="—">
            {fmt(shown()!)}
          </Show>
        </span>
        <Show when={props.suffix}>
          <span class="scrub-suffix">{props.suffix}</span>
        </Show>
      </div>
    </Show>
  );
}
