// Visual crop editing on the stage: drag inside to move, drag a handle to resize, lock an
// aspect ratio, step with arrow keys or numeric X/Y/W/H fields, then Apply (Enter) or Cancel (Esc).
import { For, onMount } from "solid-js";
import { useEditor } from "../editor/context";
import type { Editor } from "../editor/createEditor";
import { t } from "../i18n";
import { Icon } from "../icons";
import ScrubField from "../ScrubField";
import type { CropRect } from "../types";
import { Seg } from "../ui";

const LOCKS = (): { id: string; label: string; ratio: number | null }[] => [
  { id: "free", label: t("Free"), ratio: null },
  { id: "16:9", label: "16:9", ratio: 16 / 9 },
  { id: "4:3", label: "4:3", ratio: 4 / 3 },
  { id: "1:1", label: "1:1", ratio: 1 },
  { id: "4:5", label: "4:5", ratio: 4 / 5 },
  { id: "9:16", label: "9:16", ratio: 9 / 16 },
];

const HANDLES: { hx: number; hy: number; label: () => string }[] = [
  { hx: -1, hy: -1, label: () => t("Top-left crop handle") },
  { hx: 0, hy: -1, label: () => t("Top crop handle") },
  { hx: 1, hy: -1, label: () => t("Top-right crop handle") },
  { hx: -1, hy: 0, label: () => t("Left crop handle") },
  { hx: 1, hy: 0, label: () => t("Right crop handle") },
  { hx: -1, hy: 1, label: () => t("Bottom-left crop handle") },
  { hx: 0, hy: 1, label: () => t("Bottom crop handle") },
  { hx: 1, hy: 1, label: () => t("Bottom-right crop handle") },
];

const MIN = 0.05;
const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));
const pct = (v: number) => `${v * 100}%`;

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

/** Apply a move (hx=0, hy=0) or handle resize delta to `orig`, respecting optional aspect `ratio`. */
function applyCropDelta(
  orig: CropRect,
  hx: number,
  hy: number,
  dx: number,
  dy: number,
  ratio: number | null,
  src: number,
): CropRect | null {
  if (hx === 0 && hy === 0) {
    return {
      ...orig,
      x: clamp(orig.x + dx, 0, 1 - orig.w),
      y: clamp(orig.y + dy, 0, 1 - orig.h),
    };
  }
  let x0 = orig.x;
  let y0 = orig.y;
  let x1 = orig.x + orig.w;
  let y1 = orig.y + orig.h;
  if (hx < 0) x0 = clamp(orig.x + dx, 0, x1 - MIN);
  if (hx > 0) x1 = clamp(x1 + dx, x0 + MIN, 1);
  if (hy < 0) y0 = clamp(orig.y + dy, 0, y1 - MIN);
  if (hy > 0) y1 = clamp(y1 + dy, y0 + MIN, 1);
  if (ratio) {
    // Keep the ratio: whichever axis actually moved drives; the other grows away from its anchor.
    let w = x1 - x0;
    let h = y1 - y0;
    const driveX = hx !== 0 && (hy === 0 || Math.abs(dx) >= Math.abs(dy));
    if (driveX) h = (w * src) / ratio;
    else w = (h * ratio) / src;
    if (w < MIN || h < MIN) return null;
    if (hy < 0) y0 = y1 - h;
    else y1 = y0 + h;
    if (hx < 0) x0 = x1 - w;
    else x1 = x0 + w;
    if (x0 < 0 || y0 < 0 || x1 > 1 || y1 > 1) return null;
  }
  return { x: x0, y: y0, w: x1 - x0, h: y1 - y0 };
}

/** The draggable and keyboard-steppable crop rect, drawn inside the stage frame. */
export default function CropEditor(props: { lock: number | null }) {
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
    const next = applyCropDelta(orig, hx, hy, p.x - drag.start.x, p.y - drag.start.y, props.lock, ed.frameAspect());
    if (next) ed.setCropDraft(next);
  };
  const onUp = () => {
    drag = null;
  };
  const onArrowKey = (hx: number, hy: number) => (e: KeyboardEvent) => {
    let dx = 0;
    let dy = 0;
    const step = e.shiftKey ? 0.05 : 0.01;
    if (e.key === "ArrowLeft") dx = -step;
    else if (e.key === "ArrowRight") dx = step;
    else if (e.key === "ArrowUp") dy = -step;
    else if (e.key === "ArrowDown") dy = step;
    else return;
    e.preventDefault();
    e.stopPropagation();
    if (hx !== 0 || hy !== 0) {
      if (hx === 0 && dy === 0) return;
      if (hy === 0 && dx === 0) return;
    }
    const next = applyCropDelta(r(), hx, hy, dx, dy, props.lock, ed.frameAspect());
    if (next) ed.setCropDraft(next);
  };
  const cursor = (hx: number, hy: number) =>
    hx === 0 ? "ns-resize" : hy === 0 ? "ew-resize" : hx === hy ? "nwse-resize" : "nesw-resize";

  return (
    <div class="crop-editor" onPointerMove={onMove} onPointerUp={onUp} onLostPointerCapture={onUp}>
      {/* Four bounded dim rectangles around the crop area (zero 100vmax spread shadow). */}
      <div class="crop-dim" aria-hidden="true" style={{ left: "0", top: "0", right: "0", height: pct(r().y) }} />
      <div class="crop-dim" aria-hidden="true" style={{ left: "0", top: pct(r().y + r().h), right: "0", bottom: "0" }} />
      <div
        class="crop-dim"
        aria-hidden="true"
        style={{ left: "0", top: pct(r().y), width: pct(r().x), height: pct(r().h) }}
      />
      <div
        class="crop-dim"
        aria-hidden="true"
        style={{ left: pct(r().x + r().w), top: pct(r().y), right: "0", height: pct(r().h) }}
      />

      {/* biome-ignore lint/a11y/useSemanticElements: interactive crop frame with arrow-key positioning */}
      <div
        class="crop-rect"
        role="group"
        tabindex={0}
        aria-label={t("Crop area")}
        style={{ left: pct(r().x), top: pct(r().y), width: pct(r().w), height: pct(r().h) }}
        onPointerDown={onDown(0, 0)}
        onKeyDown={onArrowKey(0, 0)}
      >
        <span class="crop-third v1" aria-hidden="true" />
        <span class="crop-third v2" aria-hidden="true" />
        <span class="crop-third h1" aria-hidden="true" />
        <span class="crop-third h2" aria-hidden="true" />
        <span class="crop-size">
          {Math.round(r().w * 100)}% × {Math.round(r().h * 100)}% · {aspectOf(r(), ed.frameAspect()).toFixed(2)}:1
        </span>
        <For each={HANDLES}>
          {(h) => (
            <button
              type="button"
              class="crop-handle"
              classList={{ edge: h.hx === 0 || h.hy === 0 }}
              aria-label={h.label()}
              style={{ left: pct((h.hx + 1) / 2), top: pct((h.hy + 1) / 2), cursor: cursor(h.hx, h.hy) }}
              onPointerDown={onDown(h.hx, h.hy)}
              onKeyDown={onArrowKey(h.hx, h.hy)}
            />
          )}
        </For>
      </div>
    </div>
  );
}

/** Aspect locks, numeric bounds + Reset / Cancel / Apply, floating under the stage frame. */
export function CropBar(props: { lock: number | null; onLockChange: (ratio: number | null) => void }) {
  const ed = useEditor();
  let barEl: HTMLDivElement | undefined;
  const r = () => ed.cropDraft();
  const lockId = () => LOCKS().find((l) => l.ratio === props.lock)?.id ?? "free";

  onMount(() => {
    queueMicrotask(() => {
      barEl?.querySelector<HTMLElement>(".seg-btn.on, button:not(:disabled)")?.focus();
    });
  });

  const setX = (v: number) => {
    const c = r();
    ed.setCropDraft({ ...c, x: clamp(v, 0, 1 - c.w) });
  };
  const setY = (v: number) => {
    const c = r();
    ed.setCropDraft({ ...c, y: clamp(v, 0, 1 - c.h) });
  };
  const setW = (v: number) => {
    const c = r();
    let w = clamp(v, MIN, 1);
    let h = c.h;
    if (props.lock) {
      const src = ed.frameAspect();
      h = (w * src) / props.lock;
      if (h > 1) {
        h = 1;
        w = (h * props.lock) / src;
      } else if (h < MIN) {
        h = MIN;
        w = (h * props.lock) / src;
      }
    }
    ed.setCropDraft({ x: clamp(c.x, 0, 1 - w), y: clamp(c.y, 0, 1 - h), w, h });
  };
  const setH = (v: number) => {
    const c = r();
    let h = clamp(v, MIN, 1);
    let w = c.w;
    if (props.lock) {
      const src = ed.frameAspect();
      w = (h * props.lock) / src;
      if (w > 1) {
        w = 1;
        h = (w * src) / props.lock;
      } else if (w < MIN) {
        w = MIN;
        h = (w * src) / props.lock;
      }
    }
    ed.setCropDraft({ x: clamp(c.x, 0, 1 - w), y: clamp(c.y, 0, 1 - h), w, h });
  };

  // Let Reset/Cancel activate natively on Enter instead of bubbling to window's Apply handler.
  const keepEnterLocal = (e: KeyboardEvent) => {
    if (e.key === "Enter") e.stopPropagation();
  };

  return (
    <div ref={barEl} class="crop-bar island" role="toolbar" aria-label={t("Crop controls")}>
      <Icon name="crop" size={15} />
      <Seg
        class="crop-seg"
        label={t("Aspect ratio")}
        value={lockId()}
        options={LOCKS().map((l) => ({ value: l.id, label: l.label }))}
        onChange={(id) => {
          const ratio = LOCKS().find((l) => l.id === id)?.ratio ?? null;
          props.onLockChange(ratio);
          if (ratio) fitRatio(ed, ratio);
        }}
      />
      <div class="crop-fields">
        <div class="crop-field">
          <span class="crop-field-label" aria-hidden="true">X</span>
          <ScrubField
            value={r().x}
            min={0}
            max={Math.max(0, 1 - r().w)}
            step={0.01}
            displayScale={100}
            suffix="%"
            title={t("Crop X")}
            onInput={setX}
            onCommit={setX}
          />
        </div>
        <div class="crop-field">
          <span class="crop-field-label" aria-hidden="true">Y</span>
          <ScrubField
            value={r().y}
            min={0}
            max={Math.max(0, 1 - r().h)}
            step={0.01}
            displayScale={100}
            suffix="%"
            title={t("Crop Y")}
            onInput={setY}
            onCommit={setY}
          />
        </div>
        <div class="crop-field">
          <span class="crop-field-label" aria-hidden="true">W</span>
          <ScrubField
            value={r().w}
            min={MIN}
            max={1}
            step={0.01}
            displayScale={100}
            suffix="%"
            title={t("Crop width")}
            onInput={setW}
            onCommit={setW}
          />
        </div>
        <div class="crop-field">
          <span class="crop-field-label" aria-hidden="true">H</span>
          <ScrubField
            value={r().h}
            min={MIN}
            max={1}
            step={0.01}
            displayScale={100}
            suffix="%"
            title={t("Crop height")}
            onInput={setH}
            onCommit={setH}
          />
        </div>
      </div>
      <div class="crop-actions">
        <button
          type="button"
          class="btn sm ghost"
          data-tip={t("Back to the full frame")}
          onKeyDown={keepEnterLocal}
          onClick={() => {
            props.onLockChange(null);
            ed.setCropDraft({ x: 0, y: 0, w: 1, h: 1 });
          }}
        >
          {t("Reset")}
        </button>
        <button
          type="button"
          class="btn sm"
          data-kbd="Esc"
          onKeyDown={keepEnterLocal}
          onClick={() => void ed.finishCropEdit(false)}
        >
          {t("Cancel")}
        </button>
        <button type="button" class="btn sm primary" data-kbd="Enter" onClick={() => void ed.finishCropEdit(true)}>
          <Icon name="check" size={13} /> {t("Apply crop")}
        </button>
      </div>
    </div>
  );
}
