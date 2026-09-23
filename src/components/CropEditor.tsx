// Visual crop editing on the stage: drag inside to move, drag a handle to resize, lock an
// aspect ratio, see the rule of thirds, then Apply (Enter) or Cancel (Esc).
import { createSignal, For } from "solid-js";
import { useEditor } from "../editor/context";
import type { Editor } from "../editor/createEditor";
import { Icon } from "../icons";
import type { CropRect } from "../types";

const LOCKS: { id: string; label: string; ratio: number | null }[] = [
  { id: "free", label: "Free", ratio: null },
  { id: "16:9", label: "16:9", ratio: 16 / 9 },
  { id: "4:3", label: "4:3", ratio: 4 / 3 },
  { id: "1:1", label: "1:1", ratio: 1 },
  { id: "4:5", label: "4:5", ratio: 4 / 5 },
  { id: "9:16", label: "9:16", ratio: 9 / 16 },
];
const HANDLES: [number, number][] = [
  [-1, -1],
  [0, -1],
  [1, -1],
  [-1, 0],
  [1, 0],
  [-1, 1],
  [0, 1],
  [1, 1],
];
const MIN = 0.05;
const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));
const pct = (v: number) => `${v * 100}%`;

// The aspect lock is shared by the frame overlay and its control bar.
const [lock, setLock] = createSignal<number | null>(null);

/** Output aspect (w/h in pixels) of a normalized rect on a source of aspect `src`. */
const aspectOf = (c: CropRect, src: number) => (c.w * src) / Math.max(c.h, 1e-6);

/** Resize the draft to the largest rect of `ratio` around its center that fits the frame. */
function fitRatio(ed: Editor, ratio: number) {
  const src = ed.frameAspect();
  const c = ed.cropDraft();
  let w = c.w;
  let h = (w * src) / ratio;
  if (h > 1) {
    h = 1;
    w = (h * ratio) / src;
  }
  if (w > 1) {
    w = 1;
    h = (w * src) / ratio;
  }
  const cx = c.x + c.w / 2;
  const cy = c.y + c.h / 2;
  ed.setCropDraft({ x: clamp(cx - w / 2, 0, 1 - w), y: clamp(cy - h / 2, 0, 1 - h), w, h });
}

/** The draggable crop rect, drawn inside the stage frame. */
export default function CropEditor() {
  const ed = useEditor();
  const r = () => ed.cropDraft();

  let drag: { hx: number; hy: number; start: { x: number; y: number }; orig: CropRect } | null = null;
  const onDown = (hx: number, hy: number) => (e: PointerEvent) => {
    e.stopPropagation();
    e.preventDefault();
    (e.currentTarget as Element).setPointerCapture(e.pointerId);
    drag = { hx, hy, start: ed.norm(e), orig: { ...r() } };
  };
  const onMove = (e: PointerEvent) => {
    if (!drag) return;
    const p = ed.norm(e);
    const { hx, hy, orig } = drag;
    const dx = p.x - drag.start.x;
    const dy = p.y - drag.start.y;
    if (hx === 0 && hy === 0) {
      ed.setCropDraft({
        ...orig,
        x: clamp(orig.x + dx, 0, 1 - orig.w),
        y: clamp(orig.y + dy, 0, 1 - orig.h),
      });
      return;
    }
    let x0 = orig.x;
    let y0 = orig.y;
    let x1 = orig.x + orig.w;
    let y1 = orig.y + orig.h;
    if (hx < 0) x0 = clamp(orig.x + dx, 0, x1 - MIN);
    if (hx > 0) x1 = clamp(x1 + dx, x0 + MIN, 1);
    if (hy < 0) y0 = clamp(orig.y + dy, 0, y1 - MIN);
    if (hy > 0) y1 = clamp(y1 + dy, y0 + MIN, 1);
    const ratio = lock();
    if (ratio) {
      // Keep the ratio: the dragged axis drives, the other grows away from the anchor side.
      const src = ed.frameAspect();
      let w = x1 - x0;
      let h = y1 - y0;
      if (hx !== 0) h = (w * src) / ratio;
      else w = (h * ratio) / src;
      if (hy < 0) y0 = y1 - h;
      else y1 = y0 + h;
      if (hx < 0) x0 = x1 - w;
      else x1 = x0 + w;
      if (x0 < 0 || y0 < 0 || x1 > 1 || y1 > 1) return; // would leave the frame
    }
    ed.setCropDraft({ x: x0, y: y0, w: x1 - x0, h: y1 - y0 });
  };
  const onUp = () => {
    drag = null;
  };
  const cursor = (hx: number, hy: number) =>
    hx === 0 ? "ns-resize" : hy === 0 ? "ew-resize" : hx === hy ? "nwse-resize" : "nesw-resize";

  return (
    <div class="crop-editor" onPointerMove={onMove} onPointerUp={onUp}>
      <div
        class="crop-rect"
        style={{ left: pct(r().x), top: pct(r().y), width: pct(r().w), height: pct(r().h) }}
        onPointerDown={onDown(0, 0)}
      >
        <span class="crop-third v1" />
        <span class="crop-third v2" />
        <span class="crop-third h1" />
        <span class="crop-third h2" />
        <span class="crop-size">
          {Math.round(r().w * 100)}% × {Math.round(r().h * 100)}% · {aspectOf(r(), ed.frameAspect()).toFixed(2)}:1
        </span>
        <For each={HANDLES}>
          {([hx, hy]) => (
            <span
              class="crop-handle"
              classList={{ edge: hx === 0 || hy === 0 }}
              style={{ left: pct((hx + 1) / 2), top: pct((hy + 1) / 2), cursor: cursor(hx, hy) }}
              onPointerDown={onDown(hx, hy)}
            />
          )}
        </For>
      </div>
    </div>
  );
}

/** Aspect locks + Reset / Cancel / Apply, floating under the stage frame. */
export function CropBar() {
  const ed = useEditor();
  return (
    <div class="crop-bar">
      <Icon name="crop" size={15} />
      <div class="crop-locks">
        <For each={LOCKS}>
          {(l) => (
            <button
              type="button"
              class="crop-lock"
              classList={{ on: lock() === l.ratio }}
              aria-pressed={lock() === l.ratio}
              onClick={() => {
                setLock(l.ratio);
                if (l.ratio) fitRatio(ed, l.ratio);
              }}
            >
              {l.label}
            </button>
          )}
        </For>
      </div>
      <button
        type="button"
        class="btn sm ghost"
        data-tip="Back to the full frame"
        onClick={() => {
          setLock(null);
          ed.setCropDraft({ x: 0, y: 0, w: 1, h: 1 });
        }}
      >
        Reset
      </button>
      <button type="button" class="btn sm" data-kbd="Esc" onClick={() => void ed.finishCropEdit(false)}>
        Cancel
      </button>
      <button type="button" class="btn sm primary" data-kbd="Enter" onClick={() => void ed.finishCropEdit(true)}>
        <Icon name="check" size={13} /> Apply crop
      </button>
    </div>
  );
}
