import { createSignal, Show, type JSX } from "solid-js";

/**
 * A numeric field you can **drag to scrub** or **click to type** — the core inspector
 * ergonomic borrowed from pro editors. Horizontal drag changes the value; hold Shift for
 * a coarse (×8) sweep or Ctrl/Cmd for fine (×0.15) control. `onInput` fires live during
 * the gesture (for preview); `onCommit` fires once at the end (the undo boundary). A
 * `null` value renders an em-dash for mixed/unknown selections.
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
      // A plain click (no drag) → switch to type mode.
      setDraft(props.value === null ? "" : fmt(props.value));
      setEditing(true);
    }
  };

  const commitEdit = (raw: string) => {
    setEditing(false);
    const cleaned = raw.replace(/[^0-9.+-]/g, "").trim();
    if (cleaned === "") return;
    const parsed = Number(cleaned);
    if (!Number.isFinite(parsed)) return;
    props.onCommit(clamp(snap(parsed / scale()), props.min, props.max));
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
            el.select();
          })}
          onInput={(e) => setDraft(e.currentTarget.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              commitEdit(e.currentTarget.value);
            } else if (e.key === "Escape") {
              e.preventDefault();
              setEditing(false);
            }
          }}
          onBlur={(e) => commitEdit(e.currentTarget.value)}
        />
      }
    >
      <div
        class="scrub"
        classList={{ disabled: !!props.disabled, mixed: shown() === null }}
        title={props.title}
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
